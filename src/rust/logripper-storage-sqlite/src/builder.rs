//! Builder for configuring the `SQLite` storage adapter.

use crate::migrations::INITIAL_SCHEMA;
use crate::SqliteStorage;
use logripper_core::storage::StorageError;
use rusqlite::Connection;
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
            path: Some(PathBuf::from("logripper.db")),
            busy_timeout: Duration::from_secs(5),
        }
    }
}

impl SqliteStorageBuilder {
    /// Create a builder that targets `logripper.db` in the current working directory.
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
        let connection = match self.path.as_ref() {
            Some(path) => {
                ensure_parent_directory(path)?;
                Connection::open(path).map_err(|err| StorageError::backend(err.to_string()))?
            }
            None => Connection::open_in_memory()
                .map_err(|err| StorageError::backend(err.to_string()))?,
        };

        connection
            .busy_timeout(self.busy_timeout)
            .map_err(|err| StorageError::backend(err.to_string()))?;
        connection
            .pragma_update(None, "foreign_keys", "ON")
            .map_err(|err| StorageError::backend(err.to_string()))?;
        if self.path.is_some() {
            connection
                .pragma_update(None, "journal_mode", "WAL")
                .map_err(|err| StorageError::backend(err.to_string()))?;
        }
        connection
            .execute_batch(INITIAL_SCHEMA)
            .map_err(|err| StorageError::backend(err.to_string()))?;

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
