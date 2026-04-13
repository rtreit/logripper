//! Storage ports, errors, and query types for engine-owned persistence.

mod error;
mod ports;
mod query;

pub use error::StorageError;
pub use ports::{EngineStorage, LogbookStore, LookupSnapshotStore};
pub use query::{LogbookCounts, LookupSnapshot, QsoListQuery, QsoSortOrder, SyncMetadata};
