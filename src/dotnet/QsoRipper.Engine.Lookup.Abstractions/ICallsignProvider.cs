namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Performs a callsign lookup against a single external provider (e.g. QRZ XML).
/// </summary>
public interface ICallsignProvider
{
    /// <summary>Look up a single callsign (already normalized to uppercase).</summary>
    Task<ProviderLookupResult> LookupAsync(string callsign, CancellationToken ct = default);

    /// <summary>Human-readable name for diagnostics and logging.</summary>
    string ProviderName { get; }
}

/// <summary>Result of a single provider lookup attempt.</summary>
public sealed record ProviderLookupResult
{
    public required ProviderLookupState State { get; init; }

    /// <summary>Populated when <see cref="State"/> is <see cref="ProviderLookupState.Found"/>.</summary>
    public QsoRipper.Domain.CallsignRecord? Record { get; init; }

    /// <summary>Human-readable error detail for non-Found states.</summary>
    public string? ErrorMessage { get; init; }
}

/// <summary>Outcome of a provider lookup.</summary>
public enum ProviderLookupState
{
    Found,
    NotFound,
    AuthenticationError,
    SessionError,
    NetworkError,
}
