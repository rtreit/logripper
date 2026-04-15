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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::StorageError;

    #[test]
    fn duplicate_creates_error_with_entity_and_key() {
        let err = StorageError::duplicate("QSO", "abc-123");
        match err {
            StorageError::Duplicate { entity, key } => {
                assert_eq!(entity, "QSO");
                assert_eq!(key, "abc-123");
            }
            _ => panic!("expected Duplicate variant"),
        }
    }

    #[test]
    fn duplicate_display_contains_entity_and_key() {
        let err = StorageError::duplicate("Station", "K7ABC");
        let display = err.to_string();
        assert!(display.contains("Station"));
        assert!(display.contains("K7ABC"));
    }

    #[test]
    fn backend_creates_error_with_message() {
        let err = StorageError::backend("disk full");
        match err {
            StorageError::Backend(msg) => assert_eq!(msg, "disk full"),
            _ => panic!("expected Backend variant"),
        }
    }

    #[test]
    fn backend_display_contains_message() {
        let err = StorageError::backend("connection lost");
        assert!(err.to_string().contains("connection lost"));
    }
}
