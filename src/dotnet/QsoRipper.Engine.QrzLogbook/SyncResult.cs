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

    /// <summary>
    /// Authoritative QSO count reported by QRZ at the end of sync (Phase 3 STATUS call).
    /// <c>null</c> when the STATUS call failed; callers should fall back to the local count.
    /// </summary>
    public uint? RemoteQsoCount { get; init; }

    /// <summary>
    /// QRZ logbook owner callsign reported by Phase 3 STATUS.
    /// <c>null</c> when STATUS failed or returned no owner.
    /// </summary>
    public string? RemoteOwner { get; init; }

    /// <summary>Semicolon-delimited error messages from partial failures, or <c>null</c> when clean.</summary>
    public string? ErrorSummary { get; init; }
}
