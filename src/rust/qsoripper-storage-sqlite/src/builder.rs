//! Builder for configuring the `SQLite` storage adapter.

use crate::migrations::{HISTORY_INDEX_MIGRATION, INITIAL_SCHEMA, SOFT_DELETE_MIGRATION};
use crate::SqliteStorage;
use qsoripper_core::storage::StorageError;
use sqlite::{Connection, ConnectionThreadSafe, State, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

/// Configures SQLite-backed storage for the engine.
#[derive(Debug, Clone)]
pub struct SqliteStorageBuilder {
    path: Option<PathBuf>,
    busy_timeout: Duration,
}

impl Default for SqliteStorageBuilder {
    fn default() -> Self {
        Self {
            path: Some(PathBuf::from("qsoripper.db")),
            busy_timeout: Duration::from_secs(5),
        }
    }
}

impl SqliteStorageBuilder {
    /// Create a builder that targets `qsoripper.db` in the current working directory.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Store the database at the provided filesystem path.
    #[must_use]
    pub fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    /// Use an in-memory `SQLite` database.
    #[must_use]
    pub fn in_memory(mut self) -> Self {
        self.path = None;
        self
    }

    /// Override the busy timeout used for `SQLite` write contention.
    #[must_use]
    pub fn busy_timeout(mut self, timeout: Duration) -> Self {
        self.busy_timeout = timeout;
        self
    }

    /// Open the database, apply PRAGMAs, run migrations, and return the storage backend.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] when the database cannot be opened, configured,
    /// or migrated.
    pub fn build(self) -> Result<SqliteStorage, StorageError> {
        let mut connection = match self.path.as_ref() {
            Some(path) => {
                ensure_parent_directory(path)?;
                Connection::open_thread_safe(path).map_err(map_sqlite_error)?
            }
            None => Connection::open_thread_safe(":memory:").map_err(map_sqlite_error)?,
        };

        let timeout_ms = usize::try_from(self.busy_timeout.as_millis()).unwrap_or(usize::MAX);
        connection
            .set_busy_timeout(timeout_ms)
            .map_err(map_sqlite_error)?;
        connection
            .execute("PRAGMA foreign_keys = ON;")
            .map_err(map_sqlite_error)?;
        if self.path.is_some() {
            connection
                .execute("PRAGMA journal_mode = WAL;")
                .map_err(map_sqlite_error)?;
        }
        connection
            .execute(INITIAL_SCHEMA)
            .map_err(map_sqlite_error)?;
        apply_soft_delete_migration(&connection)?;
        connection
            .execute(HISTORY_INDEX_MIGRATION)
            .map_err(map_sqlite_error)?;

        Ok(SqliteStorage {
            connection: Mutex::new(connection),
        })
    }
}

/// Apply the soft-delete migration only when the columns are missing.
/// `SQLite` has no `ALTER TABLE ADD COLUMN IF NOT EXISTS`, so we probe
/// `pragma_table_info` first and skip the migration on databases that
/// already carry the columns.
fn apply_soft_delete_migration(connection: &ConnectionThreadSafe) -> Result<(), StorageError> {
    if has_column(connection, "qsos", "deleted_at_ms")?
        && has_column(connection, "qsos", "pending_remote_delete")?
    {
        // Index creation is `IF NOT EXISTS` already and cheap, but skipping
        // the entire script avoids the `ALTER TABLE` failure path.
        connection
            .execute("CREATE INDEX IF NOT EXISTS idx_qsos_deleted_at_ms ON qsos (deleted_at_ms);")
            .map_err(map_sqlite_error)?;
        return Ok(());
    }

    connection
        .execute(SOFT_DELETE_MIGRATION)
        .map_err(map_sqlite_error)?;
    Ok(())
}

fn has_column(
    connection: &ConnectionThreadSafe,
    table: &str,
    column: &str,
) -> Result<bool, StorageError> {
    let mut statement = connection
        .prepare("SELECT 1 FROM pragma_table_info(?) WHERE name = ? LIMIT 1")
        .map_err(map_sqlite_error)?;
    statement
        .bind((1, Value::String(table.to_string())))
        .map_err(map_sqlite_error)?;
    statement
        .bind((2, Value::String(column.to_string())))
        .map_err(map_sqlite_error)?;
    let state = statement.next().map_err(map_sqlite_error)?;
    Ok(matches!(state, State::Row))
}

fn ensure_parent_directory(path: &Path) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| StorageError::backend(err.to_string()))?;
        }
    }

    Ok(())
}

fn map_sqlite_error(error: sqlite::Error) -> StorageError {
    let message = match (error.code, error.message) {
        (Some(code), Some(message)) => format!("{message} (code {code})"),
        (Some(code), None) => format!("an SQLite error (code {code})"),
        (None, Some(message)) => message,
        (None, None) => "an SQLite error".to_string(),
    };

    StorageError::backend(message)
}
