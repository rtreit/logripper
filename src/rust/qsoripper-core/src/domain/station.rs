//! Helpers for composing durable local-station profile and snapshot data.

use crate::proto::qsoripper::domain::{QsoRecord, StationProfile, StationSnapshot};

/// Convert an active station profile into a saved station snapshot.
#[must_use]
pub fn station_snapshot_from_profile(profile: &StationProfile) -> Option<StationSnapshot> {
    let mut snapshot = StationSnapshot {
        profile_name: normalize_optional_string(profile.profile_name.as_deref()),
        station_callsign: normalize_required_string(profile.station_callsign.as_str()),
        operator_callsign: normalize_optional_string(profile.operator_callsign.as_deref()),
        operator_name: normalize_optional_string(profile.operator_name.as_deref()),
        grid: normalize_optional_string(profile.grid.as_deref()),
        county: normalize_optional_string(profile.county.as_deref()),
        state: normalize_optional_string(profile.state.as_deref()),
        country: normalize_optional_string(profile.country.as_deref()),
        dxcc: profile.dxcc.filter(|value| *value > 0),
        cq_zone: profile.cq_zone.filter(|value| *value > 0),
        itu_zone: profile.itu_zone.filter(|value| *value > 0),
        latitude: profile.latitude,
        longitude: profile.longitude,
        arrl_section: normalize_optional_string(profile.arrl_section.as_deref()),
    };
    normalize_station_snapshot(&mut snapshot);
    station_snapshot_has_values(&snapshot).then_some(snapshot)
}

/// Return the effective station snapshot for a QSO, if one is available.
#[must_use]
pub fn effective_station_snapshot(qso: &QsoRecord) -> Option<StationSnapshot> {
    let mut snapshot = qso.station_snapshot.clone().unwrap_or_default();
    let station_callsign = normalize_required_string(qso.station_callsign.as_str());
    if !station_callsign.is_empty() {
        snapshot.station_callsign = station_callsign;
    }
    normalize_station_snapshot(&mut snapshot);
    station_snapshot_has_values(&snapshot).then_some(snapshot)
}

/// Compose the saved station snapshot for a newly logged QSO.
pub fn materialize_station_snapshot_for_create(
    qso: &mut QsoRecord,
    active_profile: Option<&StationProfile>,
) {
    let mut snapshot = active_profile
        .and_then(station_snapshot_from_profile)
        .unwrap_or_default();

    if let Some(request_snapshot) = qso.station_snapshot.as_ref() {
        merge_station_snapshot(&mut snapshot, request_snapshot, false);
    }

    let effective_callsign = trimmed_non_empty(qso.station_callsign.as_str())
        .or_else(|| {
            qso.station_snapshot
                .as_ref()
                .and_then(|snapshot| trimmed_non_empty(snapshot.station_callsign.as_str()))
        })
        .or_else(|| trimmed_non_empty(snapshot.station_callsign.as_str()))
        .unwrap_or_default();

    qso.station_callsign = effective_callsign.clone();
    if !effective_callsign.is_empty() {
        snapshot.station_callsign = effective_callsign;
    }

    normalize_station_snapshot(&mut snapshot);
    qso.station_snapshot = station_snapshot_has_values(&snapshot).then_some(snapshot);
}

/// Compose the saved station snapshot for an updated QSO.
pub fn materialize_station_snapshot_for_update(qso: &mut QsoRecord, existing: Option<&QsoRecord>) {
    let mut snapshot = existing
        .and_then(effective_station_snapshot)
        .unwrap_or_default();

    if let Some(request_snapshot) = qso.station_snapshot.as_ref() {
        merge_station_snapshot(&mut snapshot, request_snapshot, true);
    }

    let effective_callsign = trimmed_non_empty(qso.station_callsign.as_str())
        .or_else(|| trimmed_non_empty(snapshot.station_callsign.as_str()))
        .unwrap_or_default();

    qso.station_callsign = effective_callsign.clone();
    if !effective_callsign.is_empty() {
        snapshot.station_callsign = effective_callsign;
    }

    normalize_station_snapshot(&mut snapshot);
    qso.station_snapshot = station_snapshot_has_values(&snapshot).then_some(snapshot);
}

/// Return whether a station profile carries any meaningful values.
#[must_use]
pub fn station_profile_has_values(profile: &StationProfile) -> bool {
    trimmed_non_empty(profile.station_callsign.as_str()).is_some()
        || profile
            .profile_name
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile
            .operator_callsign
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile
            .operator_name
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile
            .grid
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile
            .county
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile
            .state
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile
            .country
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || profile.dxcc.is_some()
        || profile.cq_zone.is_some()
        || profile.itu_zone.is_some()
        || profile.latitude.is_some()
        || profile.longitude.is_some()
        || profile
            .arrl_section
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
}

/// Return whether a station snapshot carries any meaningful values.
#[must_use]
pub fn station_snapshot_has_values(snapshot: &StationSnapshot) -> bool {
    trimmed_non_empty(snapshot.station_callsign.as_str()).is_some()
        || snapshot
            .profile_name
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot
            .operator_callsign
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot
            .operator_name
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot
            .grid
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot
            .county
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot
            .state
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot
            .country
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
        || snapshot.dxcc.is_some()
        || snapshot.cq_zone.is_some()
        || snapshot.itu_zone.is_some()
        || snapshot.latitude.is_some()
        || snapshot.longitude.is_some()
        || snapshot
            .arrl_section
            .as_deref()
            .and_then(trimmed_non_empty)
            .is_some()
}

fn merge_station_snapshot(
    base: &mut StationSnapshot,
    overlay: &StationSnapshot,
    clear_blank_strings: bool,
) {
    merge_optional_string(
        &mut base.profile_name,
        overlay.profile_name.as_deref(),
        clear_blank_strings,
    );
    if let Some(station_callsign) = trimmed_non_empty(overlay.station_callsign.as_str()) {
        base.station_callsign = station_callsign;
    }
    merge_optional_string(
        &mut base.operator_callsign,
        overlay.operator_callsign.as_deref(),
        clear_blank_strings,
    );
    merge_optional_string(
        &mut base.operator_name,
        overlay.operator_name.as_deref(),
        clear_blank_strings,
    );
    merge_optional_string(&mut base.grid, overlay.grid.as_deref(), clear_blank_strings);
    merge_optional_string(
        &mut base.county,
        overlay.county.as_deref(),
        clear_blank_strings,
    );
    merge_optional_string(
        &mut base.state,
        overlay.state.as_deref(),
        clear_blank_strings,
    );
    merge_optional_string(
        &mut base.country,
        overlay.country.as_deref(),
        clear_blank_strings,
    );

    if let Some(dxcc) = overlay.dxcc.filter(|value| *value > 0) {
        base.dxcc = Some(dxcc);
    }
    if let Some(cq_zone) = overlay.cq_zone.filter(|value| *value > 0) {
        base.cq_zone = Some(cq_zone);
    }
    if let Some(itu_zone) = overlay.itu_zone.filter(|value| *value > 0) {
        base.itu_zone = Some(itu_zone);
    }
    if let Some(latitude) = overlay.latitude {
        base.latitude = Some(latitude);
    }
    if let Some(longitude) = overlay.longitude {
        base.longitude = Some(longitude);
    }
    merge_optional_string(
        &mut base.arrl_section,
        overlay.arrl_section.as_deref(),
        clear_blank_strings,
    );
}

fn normalize_station_snapshot(snapshot: &mut StationSnapshot) {
    snapshot.profile_name = normalize_optional_string(snapshot.profile_name.as_deref());
    snapshot.station_callsign = normalize_required_string(snapshot.station_callsign.as_str());
    snapshot.operator_callsign = normalize_optional_string(snapshot.operator_callsign.as_deref());
    snapshot.operator_name = normalize_optional_string(snapshot.operator_name.as_deref());
    snapshot.grid = normalize_optional_string(snapshot.grid.as_deref());
    snapshot.county = normalize_optional_string(snapshot.county.as_deref());
    snapshot.state = normalize_optional_string(snapshot.state.as_deref());
    snapshot.country = normalize_optional_string(snapshot.country.as_deref());
    snapshot.dxcc = snapshot.dxcc.filter(|value| *value > 0);
    snapshot.cq_zone = snapshot.cq_zone.filter(|value| *value > 0);
    snapshot.itu_zone = snapshot.itu_zone.filter(|value| *value > 0);
    snapshot.arrl_section = normalize_optional_string(snapshot.arrl_section.as_deref());
}

fn merge_optional_string(
    target: &mut Option<String>,
    source: Option<&str>,
    clear_blank_strings: bool,
) {
    match source.and_then(trimmed_non_empty) {
        Some(value) => *target = Some(value),
        None if clear_blank_strings && source.is_some() => *target = None,
        None => {}
    }
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    value.and_then(trimmed_non_empty)
}

fn normalize_required_string(value: &str) -> String {
    value.trim().to_string()
}

fn trimmed_non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn create_materializes_snapshot_from_active_profile() {
        let profile = StationProfile {
            profile_name: Some("Base".to_string()),
            station_callsign: "K7RND".to_string(),
            grid: Some("CN87".to_string()),
            cq_zone: Some(3),
            arrl_section: Some("WWA".to_string()),
            ..StationProfile::default()
        };
        let mut qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            ..QsoRecord::default()
        };

        materialize_station_snapshot_for_create(&mut qso, Some(&profile));

        let snapshot = qso.station_snapshot.expect("snapshot");
        assert_eq!("K7RND", qso.station_callsign);
        assert_eq!(Some("Base"), snapshot.profile_name.as_deref());
        assert_eq!("K7RND", snapshot.station_callsign);
        assert_eq!(Some("CN87"), snapshot.grid.as_deref());
        assert_eq!(Some(3), snapshot.cq_zone);
        assert_eq!(Some("WWA"), snapshot.arrl_section.as_deref());
    }

    #[test]
    fn request_snapshot_overrides_active_profile_on_create() {
        let profile = StationProfile {
            station_callsign: "K7RND".to_string(),
            grid: Some("CN87".to_string()),
            ..StationProfile::default()
        };
        let mut qso = QsoRecord {
            station_callsign: "N0CALL".to_string(),
            worked_callsign: "W1AW".to_string(),
            station_snapshot: Some(StationSnapshot {
                grid: Some("DM79".to_string()),
                county: Some("Denver".to_string()),
                ..StationSnapshot::default()
            }),
            ..QsoRecord::default()
        };

        materialize_station_snapshot_for_create(&mut qso, Some(&profile));

        let snapshot = qso.station_snapshot.expect("snapshot");
        assert_eq!("N0CALL", snapshot.station_callsign);
        assert_eq!(Some("DM79"), snapshot.grid.as_deref());
        assert_eq!(Some("Denver"), snapshot.county.as_deref());
    }

    #[test]
    fn update_preserves_existing_snapshot_when_request_is_silent() {
        let existing = QsoRecord {
            local_id: "existing".to_string(),
            station_callsign: "K7RND".to_string(),
            worked_callsign: "W1AW".to_string(),
            station_snapshot: Some(StationSnapshot {
                profile_name: Some("Home".to_string()),
                station_callsign: "K7RND".to_string(),
                grid: Some("CN87".to_string()),
                ..StationSnapshot::default()
            }),
            ..QsoRecord::default()
        };
        let mut qso = QsoRecord {
            local_id: "existing".to_string(),
            station_callsign: "K7RND".to_string(),
            worked_callsign: "W1AW".to_string(),
            ..QsoRecord::default()
        };

        materialize_station_snapshot_for_update(&mut qso, Some(&existing));

        let snapshot = qso.station_snapshot.expect("snapshot");
        assert_eq!(Some("Home"), snapshot.profile_name.as_deref());
        assert_eq!(Some("CN87"), snapshot.grid.as_deref());
    }

    #[test]
    fn update_allows_clearing_optional_string_fields() {
        let existing = QsoRecord {
            local_id: "existing".to_string(),
            station_callsign: "K7RND".to_string(),
            worked_callsign: "W1AW".to_string(),
            station_snapshot: Some(StationSnapshot {
                station_callsign: "K7RND".to_string(),
                operator_name: Some("Randy".to_string()),
                ..StationSnapshot::default()
            }),
            ..QsoRecord::default()
        };
        let mut qso = QsoRecord {
            local_id: "existing".to_string(),
            station_callsign: "K7RND".to_string(),
            worked_callsign: "W1AW".to_string(),
            station_snapshot: Some(StationSnapshot {
                operator_name: Some("   ".to_string()),
                ..StationSnapshot::default()
            }),
            ..QsoRecord::default()
        };

        materialize_station_snapshot_for_update(&mut qso, Some(&existing));

        let snapshot = qso.station_snapshot.expect("snapshot");
        assert_eq!(None, snapshot.operator_name);
    }
}
