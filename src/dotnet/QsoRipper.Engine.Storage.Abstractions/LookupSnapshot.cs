using QsoRipper.Domain;

namespace QsoRipper.Engine.Storage;

/// <summary>
/// A cached callsign lookup result with storage timestamps.
/// </summary>
public sealed record LookupSnapshot
{
    /// <summary>The normalized callsign this snapshot is keyed by.</summary>
    public required string Callsign { get; init; }

    /// <summary>The cached lookup result.</summary>
    public required LookupResult Result { get; init; }

    /// <summary>When this snapshot was stored or last refreshed.</summary>
    public required DateTimeOffset StoredAt { get; init; }

    /// <summary>Optional expiration time after which the snapshot should be refreshed.</summary>
    public DateTimeOffset? ExpiresAt { get; init; }
}
