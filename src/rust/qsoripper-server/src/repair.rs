//! One-shot data-repair helpers run during engine startup.
//!
//! Older builds of the Rust engine never extracted `APP_QRZLOG_LOGID` from
//! ADIF responses into the dedicated [`QsoRecord::qrz_logid`] field. As a
//! result, every QRZ pull persisted records with `qrz_logid = None`, which
//! caused the QRZ sync layer to fall back to fuzzy
//! callsign+band+mode+timestamp matching and produced duplicate rows
//! whenever any of those keys drifted between the local entry and the
//! QRZ-returned ADIF.
//!
//! This module performs a startup sweep over the persisted logbook that:
//!
//! 1. Backfills `qrz_logid` from the historical
//!    `extra_fields["APP_QRZLOG_LOGID"]` / `extra_fields["APP_QRZ_LOGID"]`
//!    aliases that the old mapper left behind.
//! 2. Collapses any remaining duplicate rows that share the same
//!    `qrz_logid`, keeping the row with the earliest `created_at` (or, as a
//!    tie-break, the lexicographically smallest `local_id`) and deleting
//!    the rest. The kept row inherits any non-empty fields from the rows
//!    being removed so user-edited data is preserved when possible.
//!
//! Both steps are idempotent — running them on a clean store is a no-op.

use std::collections::HashMap;

use qsoripper_core::proto::qsoripper::domain::QsoRecord;
use qsoripper_core::storage::{LogbookStore, QsoListQuery, StorageError};

/// Extra-field keys to consult when backfilling `qrz_logid` from records
/// persisted before the ADIF mapper recognised them.
const QRZ_LOGID_EXTRA_FIELD_KEYS: &[&str] = &["APP_QRZLOG_LOGID", "APP_QRZ_LOGID"];

/// Aggregate counters describing what the repair pass touched.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct RepairReport {
    /// Number of rows whose `qrz_logid` was populated from `extra_fields`.
    pub(crate) backfilled: usize,
    /// Number of duplicate rows removed (rows kept are not counted here).
    pub(crate) duplicates_removed: usize,
    /// Number of distinct logids that had >1 row before merging.
    pub(crate) merged_groups: usize,
}

impl RepairReport {
    /// Did the repair pass make any changes?
    #[must_use]
    pub(crate) fn is_no_op(&self) -> bool {
        self.backfilled == 0 && self.duplicates_removed == 0
    }
}

/// Backfill `qrz_logid` from legacy extra-field aliases and merge any rows
/// that end up sharing the same logid.
///
/// The repair pass is best-effort: errors on individual rows are logged via
/// `eprintln!` but do not abort the sweep, so a single malformed row cannot
/// keep the engine from starting.
pub(crate) async fn backfill_qrz_logids(
    store: &dyn LogbookStore,
) -> Result<RepairReport, StorageError> {
    let qsos = store.list_qsos(&QsoListQuery::default()).await?;
    let mut report = RepairReport::default();

    // ---- Step 1: backfill the dedicated field from extra_fields ----------
    let mut after_backfill: Vec<QsoRecord> = Vec::with_capacity(qsos.len());
    for mut qso in qsos {
        let needs_backfill = qso.qrz_logid.as_deref().is_none_or(str::is_empty)
            && extract_legacy_logid(&qso).is_some();

        if needs_backfill {
            if let Some(logid) = extract_legacy_logid(&qso) {
                qso.qrz_logid = Some(logid);
                for key in QRZ_LOGID_EXTRA_FIELD_KEYS {
                    qso.extra_fields.remove(*key);
                }
                match store.update_qso(&qso).await {
                    Ok(true) => report.backfilled += 1,
                    Ok(false) => {
                        eprintln!(
                            "[repair] update_qso reported no row for {} during backfill",
                            qso.local_id
                        );
                    }
                    Err(err) => {
                        eprintln!(
                            "[repair] failed to backfill qrz_logid for {}: {err}",
                            qso.local_id
                        );
                    }
                }
            }
        }
        after_backfill.push(qso);
    }

    // ---- Step 2: collapse duplicates that share a qrz_logid --------------
    let mut groups: HashMap<String, Vec<QsoRecord>> = HashMap::new();
    for qso in after_backfill {
        if let Some(logid) = qso.qrz_logid.as_deref() {
            if !logid.is_empty() {
                groups.entry(logid.to_string()).or_default().push(qso);
            }
        }
    }

    for (logid, mut rows) in groups {
        if rows.len() < 2 {
            continue;
        }
        report.merged_groups += 1;

        // Keep the oldest row (smallest created_at_ms; fall back to
        // lexicographic local_id so the choice is deterministic).
        rows.sort_by(|a, b| {
            let a_key = (
                a.created_at.as_ref().map_or(i64::MAX, |t| t.seconds),
                a.local_id.as_str(),
            );
            let b_key = (
                b.created_at.as_ref().map_or(i64::MAX, |t| t.seconds),
                b.local_id.as_str(),
            );
            a_key.cmp(&b_key)
        });

        let mut keeper = rows.remove(0);
        let mut keeper_changed = false;
        for victim in rows {
            keeper_changed |= merge_in_place(&mut keeper, &victim);
            match store.delete_qso(&victim.local_id).await {
                Ok(true) => report.duplicates_removed += 1,
                Ok(false) => {
                    eprintln!(
                        "[repair] delete_qso reported no row for duplicate {} (logid {logid})",
                        victim.local_id
                    );
                }
                Err(err) => {
                    eprintln!(
                        "[repair] failed to delete duplicate {} (logid {logid}): {err}",
                        victim.local_id
                    );
                }
            }
        }
        if keeper_changed {
            if let Err(err) = store.update_qso(&keeper).await {
                eprintln!(
                    "[repair] failed to write merged keeper {} (logid {logid}): {err}",
                    keeper.local_id
                );
            }
        }
    }

    Ok(report)
}

/// Look up a non-empty logid in the historical extra-field aliases.
fn extract_legacy_logid(qso: &QsoRecord) -> Option<String> {
    for key in QRZ_LOGID_EXTRA_FIELD_KEYS {
        if let Some(value) = qso.extra_fields.get(*key) {
            if !value.is_empty() {
                return Some(value.clone());
            }
        }
    }
    None
}

/// Merge any non-empty fields from `victim` into `keeper` without overwriting
/// values the keeper already has. Returns `true` when at least one field on
/// the keeper was filled in from the victim.
fn merge_in_place(keeper: &mut QsoRecord, victim: &QsoRecord) -> bool {
    let mut changed = false;

    macro_rules! fill_optional_string {
        ($field:ident) => {
            if keeper.$field.as_deref().is_none_or(str::is_empty) {
                if let Some(v) = victim.$field.as_deref().filter(|s| !s.is_empty()) {
                    keeper.$field = Some(v.to_owned());
                    changed = true;
                }
            }
        };
    }

    fill_optional_string!(qrz_bookid);
    fill_optional_string!(notes);
    fill_optional_string!(comment);
    fill_optional_string!(submode);
    fill_optional_string!(worked_grid);
    fill_optional_string!(worked_operator_name);

    if keeper.rst_sent.is_none() {
        if let Some(v) = victim.rst_sent.clone() {
            keeper.rst_sent = Some(v);
            changed = true;
        }
    }
    if keeper.rst_received.is_none() {
        if let Some(v) = victim.rst_received.clone() {
            keeper.rst_received = Some(v);
            changed = true;
        }
    }

    for (k, v) in &victim.extra_fields {
        if v.is_empty() {
            continue;
        }
        if !keeper.extra_fields.contains_key(k) {
            keeper.extra_fields.insert(k.clone(), v.clone());
            changed = true;
        }
    }

    changed
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use prost_types::Timestamp;
    use qsoripper_core::domain::qso::QsoRecordBuilder;
    use qsoripper_core::proto::qsoripper::domain::{Band, Mode};
    use qsoripper_core::storage::{LogbookStore, QsoListQuery};
    use qsoripper_storage_memory::MemoryStorage;

    use super::backfill_qrz_logids;

    fn qso(
        local_id: &str,
        worked: &str,
        band: Band,
        mode: Mode,
        ts: i64,
    ) -> qsoripper_core::proto::qsoripper::domain::QsoRecord {
        let mut q = QsoRecordBuilder::new("W1AW", worked)
            .band(band)
            .mode(mode)
            .timestamp(Timestamp {
                seconds: ts,
                nanos: 0,
            })
            .build();
        q.local_id = local_id.to_string();
        q.created_at = Some(Timestamp {
            seconds: ts,
            nanos: 0,
        });
        q
    }

    #[tokio::test]
    async fn backfill_populates_qrz_logid_from_canonical_extra_field() {
        let store = MemoryStorage::new();
        let mut q = qso("L1", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        q.extra_fields
            .insert("APP_QRZLOG_LOGID".into(), "987".into());
        store.insert_qso(&q).await.unwrap();

        let report = backfill_qrz_logids(&store).await.unwrap();
        assert_eq!(report.backfilled, 1);
        assert_eq!(report.duplicates_removed, 0);

        let saved = store.get_qso("L1").await.unwrap().unwrap();
        assert_eq!(saved.qrz_logid.as_deref(), Some("987"));
        assert!(
            !saved.extra_fields.contains_key("APP_QRZLOG_LOGID"),
            "backfill must remove the legacy extra-field alias once promoted"
        );
    }

    #[tokio::test]
    async fn backfill_populates_qrz_logid_from_legacy_alias() {
        let store = MemoryStorage::new();
        let mut q = qso("L1", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        q.extra_fields
            .insert("APP_QRZ_LOGID".into(), "LEGACY".into());
        store.insert_qso(&q).await.unwrap();

        let report = backfill_qrz_logids(&store).await.unwrap();
        assert_eq!(report.backfilled, 1);

        let saved = store.get_qso("L1").await.unwrap().unwrap();
        assert_eq!(saved.qrz_logid.as_deref(), Some("LEGACY"));
    }

    #[tokio::test]
    async fn backfill_skips_rows_with_existing_logid() {
        let store = MemoryStorage::new();
        let mut q = qso("L1", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        q.qrz_logid = Some("ALREADY".into());
        q.extra_fields
            .insert("APP_QRZLOG_LOGID".into(), "STALE".into());
        store.insert_qso(&q).await.unwrap();

        let report = backfill_qrz_logids(&store).await.unwrap();
        assert_eq!(report.backfilled, 0);

        let saved = store.get_qso("L1").await.unwrap().unwrap();
        assert_eq!(
            saved.qrz_logid.as_deref(),
            Some("ALREADY"),
            "existing dedicated logid takes precedence"
        );
        assert_eq!(
            saved
                .extra_fields
                .get("APP_QRZLOG_LOGID")
                .map(String::as_str),
            Some("STALE"),
            "stale extra_field should be left alone when the dedicated field is already set"
        );
    }

    #[tokio::test]
    async fn dedup_removes_duplicate_rows_sharing_a_logid() {
        let store = MemoryStorage::new();
        // Two rows that share the same qrz_logid but differ in band/mode —
        // the kind of pair fuzzy-match would have failed to dedup. The
        // older row (lower created_at) must be preserved.
        let mut a = qso("OLD", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        a.qrz_logid = Some("DUP".into());
        a.notes = Some("operator notes".into());
        let mut b = qso("NEW", "K7ABC", Band::Band40m, Mode::Cw, 1_700_010_000);
        b.qrz_logid = Some("DUP".into());
        b.created_at = Some(Timestamp {
            seconds: 1_700_010_000,
            nanos: 0,
        });

        store.insert_qso(&a).await.unwrap();
        store.insert_qso(&b).await.unwrap();

        let report = backfill_qrz_logids(&store).await.unwrap();
        assert_eq!(report.duplicates_removed, 1);
        assert_eq!(report.merged_groups, 1);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        let row = all.first().unwrap();
        assert_eq!(row.local_id, "OLD", "the older row is the canonical one");
        assert_eq!(row.qrz_logid.as_deref(), Some("DUP"));
        assert_eq!(row.notes.as_deref(), Some("operator notes"));
    }

    #[tokio::test]
    async fn dedup_after_backfill_collapses_orphaned_pairs() {
        // Models the user's actual situation: a previously-unkeyed row
        // (qrz_logid = None, but the legacy extra_field alias carries the
        // logid) lives next to a keyed row from a later sync. Backfill +
        // dedup should leave exactly one row with the dedicated logid set.
        let store = MemoryStorage::new();
        let mut old = qso("OLD", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        old.qrz_logid = None;
        old.extra_fields
            .insert("APP_QRZLOG_LOGID".into(), "DUP".into());
        let mut newer = qso("NEW", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_010_000);
        newer.qrz_logid = Some("DUP".into());
        newer.created_at = Some(Timestamp {
            seconds: 1_700_010_000,
            nanos: 0,
        });

        store.insert_qso(&old).await.unwrap();
        store.insert_qso(&newer).await.unwrap();

        let report = backfill_qrz_logids(&store).await.unwrap();
        assert_eq!(report.backfilled, 1);
        assert_eq!(report.duplicates_removed, 1);

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all.first().unwrap().qrz_logid.as_deref(), Some("DUP"));
    }

    #[tokio::test]
    async fn repair_is_idempotent_no_op_on_clean_store() {
        let store = MemoryStorage::new();
        let mut q = qso("L1", "K7ABC", Band::Band20m, Mode::Ft8, 1_700_000_000);
        q.qrz_logid = Some("L1".into());
        store.insert_qso(&q).await.unwrap();

        let r1 = backfill_qrz_logids(&store).await.unwrap();
        let r2 = backfill_qrz_logids(&store).await.unwrap();
        assert!(r1.is_no_op());
        assert!(r2.is_no_op());

        let all = store.list_qsos(&QsoListQuery::default()).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn repair_handles_empty_store() {
        let store = MemoryStorage::new();
        let report = backfill_qrz_logids(&store).await.unwrap();
        assert!(report.is_no_op());
    }
}
