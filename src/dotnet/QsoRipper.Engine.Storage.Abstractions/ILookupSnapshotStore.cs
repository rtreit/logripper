namespace QsoRipper.Engine.Storage;

/// <summary>
/// Stores cached callsign lookup snapshots.
/// Callsigns are normalized (upper-cased, trimmed) for storage and retrieval.
/// </summary>
public interface ILookupSnapshotStore
{
    /// <summary>Retrieves a cached lookup snapshot by callsign, or <c>null</c> if not found.</summary>
    ValueTask<LookupSnapshot?> GetAsync(string callsign);

    /// <summary>Creates or updates a lookup snapshot for the given callsign.</summary>
    ValueTask UpsertAsync(LookupSnapshot snapshot);

    /// <summary>Deletes a cached lookup snapshot by callsign.</summary>
    /// <returns><c>true</c> if the snapshot was found and deleted; <c>false</c> if not found.</returns>
    ValueTask<bool> DeleteAsync(string callsign);
}
