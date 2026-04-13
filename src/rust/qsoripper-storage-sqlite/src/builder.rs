//! Builder for configuring the `SQLite` storage adapter.

use crate::migrations::INITIAL_SCHEMA;
use crate::SqliteStorage;
use qsoripper_core::storage::StorageError;
use sqlite::Connection;
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

        Ok(SqliteStorage {
            connection: Mutex::new(connection),
        })
    }
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
