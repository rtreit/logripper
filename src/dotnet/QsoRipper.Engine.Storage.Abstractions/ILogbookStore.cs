using QsoRipper.Domain;

namespace QsoRipper.Engine.Storage;

/// <summary>
/// Stores QSO records, provides query/filter/pagination, and tracks sync metadata.
/// </summary>
public interface ILogbookStore
{
    /// <summary>Inserts a new QSO record. Throws <see cref="StorageException"/> on duplicate LocalId.</summary>
    ValueTask InsertQsoAsync(QsoRecord qso);

    /// <summary>Replaces an existing QSO record identified by <see cref="QsoRecord.LocalId"/>.</summary>
    /// <returns><c>true</c> if the record was found and updated; <c>false</c> if not found.</returns>
    ValueTask<bool> UpdateQsoAsync(QsoRecord qso);

    /// <summary>Deletes a QSO record by its local identifier.</summary>
    /// <returns><c>true</c> if the record was found and deleted; <c>false</c> if not found.</returns>
    ValueTask<bool> DeleteQsoAsync(string localId);

    /// <summary>
    /// Soft-deletes a QSO by setting its tombstone (<c>DeletedAt</c>) and optionally
    /// queueing it for remote QRZ delete on the next sync. The row remains in the
    /// table so it can be restored.
    /// </summary>
    /// <param name="localId">Local identifier of the QSO to soft-delete.</param>
    /// <param name="deletedAt">Wall-clock tombstone time recorded on the row.</param>
    /// <param name="pendingRemoteDelete">When <c>true</c>, mark the row for QRZ removal on the next sync.</param>
    /// <returns><c>true</c> if the record was found and updated; <c>false</c> if not found.</returns>
    ValueTask<bool> SoftDeleteQsoAsync(string localId, DateTimeOffset deletedAt, bool pendingRemoteDelete);

    /// <summary>
    /// Restores a previously soft-deleted QSO, clearing both the tombstone and
    /// the pending-remote-delete flag.
    /// </summary>
    /// <param name="localId">Local identifier of the QSO to restore.</param>
    /// <returns><c>true</c> if the record was found and restored; <c>false</c> if not found.</returns>
    ValueTask<bool> RestoreQsoAsync(string localId);

    /// <summary>Retrieves a single QSO record by its local identifier, or <c>null</c> if not found.</summary>
    ValueTask<QsoRecord?> GetQsoAsync(string localId);

    /// <summary>Queries QSO records with optional filters, sorting, and pagination.</summary>
    ValueTask<IReadOnlyList<QsoRecord>> ListQsosAsync(QsoListQuery query);

    /// <summary>Returns aggregate counts for the logbook.</summary>
    ValueTask<LogbookCounts> GetCountsAsync();

    /// <summary>
    /// Permanently removes soft-deleted QSO records from storage.
    /// </summary>
    /// <param name="localIds">When non-null, restricts purging to these local IDs only.</param>
    /// <param name="olderThan">When set, only purges rows whose <c>DeletedAt</c> is at or before this time.</param>
    /// <returns>The number of records permanently removed.</returns>
    ValueTask<int> PurgeDeletedQsosAsync(IReadOnlyList<string>? localIds, DateTimeOffset? olderThan);

    /// <summary>Retrieves the current sync metadata.</summary>
    ValueTask<SyncMetadata> GetSyncMetadataAsync();

    /// <summary>Creates or updates the sync metadata.</summary>
    ValueTask UpsertSyncMetadataAsync(SyncMetadata metadata);
}
