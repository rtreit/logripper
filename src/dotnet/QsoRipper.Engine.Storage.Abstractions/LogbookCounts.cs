namespace QsoRipper.Engine.Storage;

/// <summary>
/// Aggregate counts for a logbook store.
/// </summary>
/// <param name="LocalQsoCount">Total number of QSO records in the store.</param>
/// <param name="PendingUploadCount">Number of QSO records not yet synced (sync_status != Synced).</param>
public sealed record LogbookCounts(int LocalQsoCount, int PendingUploadCount);
