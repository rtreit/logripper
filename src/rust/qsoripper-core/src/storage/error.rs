//! Storage-layer error types shared by all engine persistence adapters.

use thiserror::Error;

/// Errors returned by engine-owned storage ports.
#[derive(Debug, Error)]
pub enum StorageError {
    /// A record already exists with the same unique key.
    #[error("{entity} '{key}' already exists.")]
    Duplicate {
        /// Logical entity type for the duplicate record.
        entity: &'static str,
        /// Unique key value that already exists.
        key: String,
    },
    /// Persisted data could not be decoded or did not match the expected shape.
    #[error("Stored data is corrupt: {0}")]
    CorruptData(String),
    /// An adapter does not support the requested operation.
    #[error("Storage operation is not supported: {0}")]
    Unsupported(String),
    /// The backend reported an operational failure.
    #[error("Storage backend failure: {0}")]
    Backend(String),
}

impl StorageError {
    /// Create a duplicate-key storage error.
    #[must_use]
    pub fn duplicate(entity: &'static str, key: impl Into<String>) -> Self {
        Self::Duplicate {
            entity,
            key: key.into(),
        }
    }

    /// Create a backend failure storage error.
    #[must_use]
    pub fn backend(message: impl Into<String>) -> Self {
        Self::Backend(message.into())
    }
}
