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

    /// <summary>Retrieves a single QSO record by its local identifier, or <c>null</c> if not found.</summary>
    ValueTask<QsoRecord?> GetQsoAsync(string localId);

    /// <summary>Queries QSO records with optional filters, sorting, and pagination.</summary>
    ValueTask<IReadOnlyList<QsoRecord>> ListQsosAsync(QsoListQuery query);

    /// <summary>Returns aggregate counts for the logbook.</summary>
    ValueTask<LogbookCounts> GetCountsAsync();

    /// <summary>Retrieves the current sync metadata.</summary>
    ValueTask<SyncMetadata> GetSyncMetadataAsync();

    /// <summary>Creates or updates the sync metadata.</summary>
    ValueTask UpsertSyncMetadataAsync(SyncMetadata metadata);
}
