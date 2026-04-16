namespace QsoRipper.Engine.QrzLogbook;

/// <summary>
/// Outcome of a <see cref="QrzSyncEngine.ExecuteSyncAsync"/> run.
/// </summary>
public sealed record SyncResult
{
    /// <summary>Number of QSOs downloaded from QRZ and merged or inserted locally.</summary>
    public uint DownloadedCount { get; init; }

    /// <summary>Number of local QSOs successfully uploaded to QRZ.</summary>
    public uint UploadedCount { get; init; }

    /// <summary>Number of conflicting QSOs detected during merge.</summary>
    public uint ConflictCount { get; init; }

    /// <summary>Semicolon-delimited error messages from partial failures, or <c>null</c> when clean.</summary>
    public string? ErrorSummary { get; init; }
}
