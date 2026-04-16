using QsoRipper.Domain;

namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Fans out callsign lookups through an <see cref="ILookupCoordinator"/> with bounded concurrency.
/// Used by the BatchLookup gRPC handler and directly testable without gRPC infrastructure.
/// </summary>
public static class BatchLookupOrchestrator
{
    /// <summary>Default maximum number of concurrent lookups.</summary>
    public const int DefaultMaxConcurrency = 5;

    /// <summary>
    /// Look up multiple callsigns in parallel with bounded concurrency.
    /// Results are returned in the same order as <paramref name="callsigns"/>.
    /// </summary>
    public static async Task<LookupResult[]> ExecuteAsync(
        ILookupCoordinator coordinator,
        IReadOnlyList<string> callsigns,
        bool skipCache = false,
        int maxConcurrency = DefaultMaxConcurrency,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(coordinator);
        ArgumentNullException.ThrowIfNull(callsigns);

        if (callsigns.Count == 0)
        {
            return [];
        }

        // Semaphore is disposed only after Task.WhenAll completes (all tasks finished).
#pragma warning disable CA2025 // Task.WhenAll ensures all tasks complete before disposal
        using var semaphore = new SemaphoreSlim(maxConcurrency);
        var tasks = new Task<LookupResult>[callsigns.Count];

        for (var i = 0; i < callsigns.Count; i++)
        {
            var callsign = callsigns[i];
            tasks[i] = ThrottledLookupAsync(coordinator, callsign, skipCache, semaphore, ct);
        }

        return await Task.WhenAll(tasks).ConfigureAwait(false);
#pragma warning restore CA2025
    }

    private static async Task<LookupResult> ThrottledLookupAsync(
        ILookupCoordinator coordinator,
        string callsign,
        bool skipCache,
        SemaphoreSlim semaphore,
        CancellationToken ct)
    {
        await semaphore.WaitAsync(ct).ConfigureAwait(false);
        try
        {
            return await coordinator.LookupAsync(callsign, skipCache, ct).ConfigureAwait(false);
        }
        finally
        {
            semaphore.Release();
        }
    }
}
