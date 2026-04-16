namespace QsoRipper.Engine.Storage;

/// <summary>
/// Tracks sync-related metadata for the logbook.
/// </summary>
public sealed record SyncMetadata
{
    /// <summary>Number of QSOs known to exist in the QRZ logbook.</summary>
    public int QrzQsoCount { get; init; }

    /// <summary>Timestamp of the most recent sync operation, or <c>null</c> if never synced.</summary>
    public DateTimeOffset? LastSync { get; init; }

    /// <summary>Callsign of the QRZ logbook owner, or <c>null</c> if unknown.</summary>
    public string? QrzLogbookOwner { get; init; }
}
