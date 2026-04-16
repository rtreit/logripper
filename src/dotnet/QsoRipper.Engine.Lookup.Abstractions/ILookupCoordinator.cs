using QsoRipper.Domain;

namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Orchestrates callsign lookups with caching, in-flight dedup, and slash-call fallback.
/// </summary>
public interface ILookupCoordinator
{
    /// <summary>Perform a unary lookup with cache and provider orchestration.</summary>
    Task<LookupResult> LookupAsync(string callsign, bool skipCache = false, CancellationToken ct = default);

    /// <summary>Return a cache-only lookup result.</summary>
    Task<LookupResult> GetCachedAsync(string callsign);

    /// <summary>Perform a streaming lookup: Loading → (Stale?) → Found/NotFound/Error.</summary>
    Task<LookupResult[]> StreamLookupAsync(string callsign, CancellationToken ct = default);
}
