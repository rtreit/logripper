//! Embedded schema migrations for the `SQLite` storage adapter.

/// Initial schema for QSO, sync metadata, and lookup snapshot persistence.
pub(crate) const INITIAL_SCHEMA: &str = include_str!("migrations/0001_initial.sql");

/// Adds soft-delete columns (`deleted_at_ms`, `pending_remote_delete`) and
/// supporting index. Applied only when the columns are missing — see
/// `builder::apply_soft_delete_migration` for the conditional execution.
pub(crate) const SOFT_DELETE_MIGRATION: &str = include_str!("migrations/0002_soft_delete.sql");

/// Adds an expression index on `UPPER(worked_callsign)` so the QSO history
/// lookup query (`UPPER(worked_callsign) = ?`) can use an index instead of
/// scanning the table.
pub(crate) const HISTORY_INDEX_MIGRATION: &str = include_str!("migrations/0003_history_index.sql");
