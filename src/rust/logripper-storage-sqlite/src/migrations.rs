//! Embedded schema migrations for the `SQLite` storage adapter.

/// Initial schema for QSO, sync metadata, and lookup snapshot persistence.
pub(crate) const INITIAL_SCHEMA: &str = include_str!("migrations/0001_initial.sql");
