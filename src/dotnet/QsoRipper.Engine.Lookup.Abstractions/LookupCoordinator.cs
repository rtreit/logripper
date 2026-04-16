using System.Collections.Concurrent;
using System.Diagnostics;
using QsoRipper.Domain;
using QsoRipper.Engine.Storage;

namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Cache-first lookup coordinator with in-flight dedup and slash-call fallback.
/// Matches the Rust coordinator's behavior.
/// </summary>
public sealed class LookupCoordinator : ILookupCoordinator
{
    private readonly ICallsignProvider _provider;
    private readonly ILookupSnapshotStore? _snapshotStore;
    private readonly TimeSpan _positiveTtl;
    private readonly TimeSpan _negativeTtl;

    private readonly ConcurrentDictionary<string, CacheEntry> _cache = new(StringComparer.Ordinal);
    private readonly ConcurrentDictionary<string, Task<ProviderLookupResult>> _inFlight = new(StringComparer.Ordinal);

    /// <summary>Create a coordinator with configurable TTLs and optional snapshot persistence.</summary>
    public LookupCoordinator(
        ICallsignProvider provider,
        ILookupSnapshotStore? snapshotStore = null,
        TimeSpan? positiveTtl = null,
        TimeSpan? negativeTtl = null)
    {
        _provider = provider ?? throw new ArgumentNullException(nameof(provider));
        _snapshotStore = snapshotStore;
        _positiveTtl = positiveTtl ?? TimeSpan.FromMinutes(15);
        _negativeTtl = negativeTtl ?? TimeSpan.FromMinutes(2);
    }

    /// <inheritdoc/>
    public async Task<LookupResult> LookupAsync(string callsign, bool skipCache = false, CancellationToken ct = default)
    {
        var normalized = NormalizeCallsign(callsign);

        if (!skipCache)
        {
            var cached = GetFreshCacheEntry(normalized);
            if (cached is not null)
            {
                return CacheEntryToResult(cached, normalized, cacheHit: true);
            }
        }

        var parsed = CallsignParser.Parse(normalized);
        var baseCallsign = parsed.Position != ModifierPosition.None ? parsed.BaseCallsign : null;

        var sw = Stopwatch.StartNew();
        var providerResult = await RunProviderLookupWithFallback(normalized, baseCallsign, ct).ConfigureAwait(false);
        var latencyMs = (uint)Math.Min(sw.ElapsedMilliseconds, uint.MaxValue);

        return await ProviderResultToLookup(providerResult, normalized, latencyMs, ct).ConfigureAwait(false);
    }

    /// <inheritdoc/>
    public Task<LookupResult> GetCachedAsync(string callsign)
    {
        var normalized = NormalizeCallsign(callsign);
        var cached = GetFreshCacheEntry(normalized);
        if (cached is not null)
        {
            return Task.FromResult(CacheEntryToResult(cached, normalized, cacheHit: true));
        }

        return Task.FromResult(new LookupResult
        {
            State = LookupState.NotFound,
            CacheHit = false,
            LookupLatencyMs = 0,
            QueriedCallsign = normalized,
        });
    }

    /// <inheritdoc/>
    public async Task<LookupResult[]> StreamLookupAsync(string callsign, CancellationToken ct = default)
    {
        var normalized = NormalizeCallsign(callsign);
        var updates = new List<LookupResult>
        {
            new()
            {
                State = LookupState.Loading,
                CacheHit = false,
                LookupLatencyMs = 0,
                QueriedCallsign = normalized,
            },
        };

        var cached = GetCacheEntry(normalized);
        if (cached is not null)
        {
            if (IsFresh(cached))
            {
                updates.Add(CacheEntryToResult(cached, normalized, cacheHit: true));
                return [.. updates];
            }

            // Stale entry (only emit stale for Found records)
            if (cached.Record is not null)
            {
                updates.Add(CacheEntryToResult(cached, normalized, cacheHit: true, stateOverride: LookupState.Stale));
            }
        }

        var parsed = CallsignParser.Parse(normalized);
        var baseCallsign = parsed.Position != ModifierPosition.None ? parsed.BaseCallsign : null;

        var sw = Stopwatch.StartNew();
        var providerResult = await RunProviderLookupWithFallback(normalized, baseCallsign, ct).ConfigureAwait(false);
        var latencyMs = (uint)Math.Min(sw.ElapsedMilliseconds, uint.MaxValue);

        updates.Add(await ProviderResultToLookup(providerResult, normalized, latencyMs, ct).ConfigureAwait(false));
        return [.. updates];
    }

    private async Task<ProviderLookupResult> RunProviderLookupWithFallback(
        string exactCallsign, string? baseCallsign, CancellationToken ct)
    {
        var first = await RunProviderLookupDeduped(exactCallsign, ct).ConfigureAwait(false);

        if (baseCallsign is not null
            && !string.Equals(baseCallsign, exactCallsign, StringComparison.Ordinal)
            && first.State == ProviderLookupState.NotFound)
        {
            return await RunProviderLookupDeduped(baseCallsign, ct).ConfigureAwait(false);
        }

        return first;
    }

    private async Task<ProviderLookupResult> RunProviderLookupDeduped(string normalizedCallsign, CancellationToken ct)
    {
        // Use GetOrAdd to ensure only one in-flight request per callsign.
        // The first caller creates the task; subsequent callers await the same task.
        var task = _inFlight.GetOrAdd(normalizedCallsign, key =>
            Task.Run(() => _provider.LookupAsync(key, ct), CancellationToken.None));

        try
        {
            return await task.ConfigureAwait(false);
        }
        finally
        {
            // Remove the in-flight entry once the task completes.
            // Only remove if the stored task is still our task (avoid removing a newer one).
            _inFlight.TryRemove(new KeyValuePair<string, Task<ProviderLookupResult>>(normalizedCallsign, task));
        }
    }

    private async Task<LookupResult> ProviderResultToLookup(
        ProviderLookupResult providerResult, string normalizedCallsign, uint latencyMs, CancellationToken ct)
    {
        switch (providerResult.State)
        {
            case ProviderLookupState.Found:
                var record = providerResult.Record!;
                StoreCacheEntry(normalizedCallsign, new CacheEntry(record.Clone(), DateTimeOffset.UtcNow));
                await PersistSnapshotAsync(normalizedCallsign, LookupState.Found, record, ct).ConfigureAwait(false);

                return new LookupResult
                {
                    State = LookupState.Found,
                    Record = record,
                    CacheHit = false,
                    LookupLatencyMs = latencyMs,
                    QueriedCallsign = normalizedCallsign,
                };

            case ProviderLookupState.NotFound:
                StoreCacheEntry(normalizedCallsign, new CacheEntry(null, DateTimeOffset.UtcNow));
                await PersistSnapshotAsync(normalizedCallsign, LookupState.NotFound, null, ct).ConfigureAwait(false);

                return new LookupResult
                {
                    State = LookupState.NotFound,
                    CacheHit = false,
                    LookupLatencyMs = latencyMs,
                    QueriedCallsign = normalizedCallsign,
                };

            default:
                return new LookupResult
                {
                    State = LookupState.Error,
                    ErrorMessage = providerResult.ErrorMessage ?? "Provider error",
                    CacheHit = false,
                    LookupLatencyMs = latencyMs,
                    QueriedCallsign = normalizedCallsign,
                };
        }
    }

    private CacheEntry? GetFreshCacheEntry(string normalizedCallsign)
    {
        var entry = GetCacheEntry(normalizedCallsign);
        return entry is not null && IsFresh(entry) ? entry : null;
    }

    private CacheEntry? GetCacheEntry(string normalizedCallsign)
    {
        return _cache.TryGetValue(normalizedCallsign, out var entry) ? entry : null;
    }

    private void StoreCacheEntry(string normalizedCallsign, CacheEntry entry)
    {
        _cache[normalizedCallsign] = entry;
    }

    private bool IsFresh(CacheEntry entry)
    {
        var ttl = entry.Record is not null ? _positiveTtl : _negativeTtl;
        return DateTimeOffset.UtcNow - entry.CachedAt <= ttl;
    }

    private static LookupResult CacheEntryToResult(
        CacheEntry entry, string normalizedCallsign, bool cacheHit, LookupState? stateOverride = null)
    {
        if (entry.Record is not null)
        {
            return new LookupResult
            {
                State = stateOverride ?? LookupState.Found,
                Record = entry.Record.Clone(),
                CacheHit = cacheHit,
                LookupLatencyMs = 0,
                QueriedCallsign = normalizedCallsign,
            };
        }

        return new LookupResult
        {
            State = LookupState.NotFound,
            CacheHit = cacheHit,
            LookupLatencyMs = 0,
            QueriedCallsign = normalizedCallsign,
        };
    }

    private async Task PersistSnapshotAsync(
        string normalizedCallsign, LookupState state, CallsignRecord? record, CancellationToken ct)
    {
        if (_snapshotStore is null)
        {
            return;
        }

        _ = ct; // Snapshot store does not accept CancellationToken.
        var snapshot = new LookupSnapshot
        {
            Callsign = normalizedCallsign,
            Result = new LookupResult
            {
                State = state,
                Record = record?.Clone(),
                QueriedCallsign = normalizedCallsign,
            },
            StoredAt = DateTimeOffset.UtcNow,
        };

        await _snapshotStore.UpsertAsync(snapshot).ConfigureAwait(false);
    }

    internal static string NormalizeCallsign(string callsign)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);
        return callsign.Trim().ToUpperInvariant();
    }

    private sealed record CacheEntry(CallsignRecord? Record, DateTimeOffset CachedAt);
}
