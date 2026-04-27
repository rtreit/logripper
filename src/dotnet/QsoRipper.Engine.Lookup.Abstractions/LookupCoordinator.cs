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
    internal const int DefaultHistoryLimit = 25;

    private readonly ICallsignProvider _provider;
    private readonly ILookupSnapshotStore? _snapshotStore;
    private readonly ILogbookStore? _logbookStore;
    private readonly TimeSpan _positiveTtl;
    private readonly TimeSpan _negativeTtl;

    private readonly ConcurrentDictionary<string, CacheEntry> _cache = new(StringComparer.Ordinal);
    private readonly ConcurrentDictionary<string, Task<ProviderLookupResult>> _inFlight = new(StringComparer.Ordinal);

    /// <summary>Create a coordinator with configurable TTLs and optional snapshot persistence.</summary>
    public LookupCoordinator(
        ICallsignProvider provider,
        ILookupSnapshotStore? snapshotStore = null,
        TimeSpan? positiveTtl = null,
        TimeSpan? negativeTtl = null,
        ILogbookStore? logbookStore = null)
    {
        _provider = provider ?? throw new ArgumentNullException(nameof(provider));
        _snapshotStore = snapshotStore;
        _logbookStore = logbookStore;
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
                var cachedResult = CacheEntryToResult(cached, normalized, cacheHit: true);
                await PopulateHistoryAsync(cachedResult, normalized).ConfigureAwait(false);
                return cachedResult;
            }
        }

        var parsed = CallsignParser.Parse(normalized);
        var baseCallsign = parsed.Position != ModifierPosition.None ? parsed.BaseCallsign : null;

        var sw = Stopwatch.StartNew();
        var providerResult = await RunProviderLookupWithFallback(normalized, baseCallsign, ct).ConfigureAwait(false);
        var latencyMs = (uint)Math.Min(sw.ElapsedMilliseconds, uint.MaxValue);

        var result = await ProviderResultToLookup(providerResult, normalized, latencyMs, ct).ConfigureAwait(false);
        await PopulateHistoryAsync(result, normalized).ConfigureAwait(false);
        return result;
    }

    /// <inheritdoc/>
    public async Task<LookupResult> GetCachedAsync(string callsign)
    {
        var normalized = NormalizeCallsign(callsign);
        var cached = GetFreshCacheEntry(normalized);
        LookupResult result;
        if (cached is not null)
        {
            result = CacheEntryToResult(cached, normalized, cacheHit: true);
        }
        else
        {
            result = new LookupResult
            {
                State = LookupState.NotFound,
                CacheHit = false,
                LookupLatencyMs = 0,
                QueriedCallsign = normalized,
            };
        }

        await PopulateHistoryAsync(result, normalized).ConfigureAwait(false);
        return result;
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
                var freshResult = CacheEntryToResult(cached, normalized, cacheHit: true);
                await PopulateHistoryAsync(freshResult, normalized).ConfigureAwait(false);
                updates.Add(freshResult);
                return [.. updates];
            }

            if (cached.Record is not null)
            {
                var staleResult = CacheEntryToResult(cached, normalized, cacheHit: true, stateOverride: LookupState.Stale);
                await PopulateHistoryAsync(staleResult, normalized).ConfigureAwait(false);
                updates.Add(staleResult);
            }
        }

        var parsed = CallsignParser.Parse(normalized);
        var baseCallsign = parsed.Position != ModifierPosition.None ? parsed.BaseCallsign : null;

        var sw = Stopwatch.StartNew();
        var providerResult = await RunProviderLookupWithFallback(normalized, baseCallsign, ct).ConfigureAwait(false);
        var latencyMs = (uint)Math.Min(sw.ElapsedMilliseconds, uint.MaxValue);

        var finalResult = await ProviderResultToLookup(providerResult, normalized, latencyMs, ct).ConfigureAwait(false);
        await PopulateHistoryAsync(finalResult, normalized).ConfigureAwait(false);
        updates.Add(finalResult);
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

    private async Task PopulateHistoryAsync(LookupResult result, string normalizedCallsign)
    {
        if (_logbookStore is null || result.State == LookupState.Loading)
        {
            return;
        }

        try
        {
            var page = await _logbookStore.ListQsoHistoryAsync(normalizedCallsign, DefaultHistoryLimit)
                .ConfigureAwait(false);

            result.PriorQsos.Clear();
            foreach (var qso in page.Entries)
            {
                result.PriorQsos.Add(QsoToHistoryEntry(qso));
            }
            result.PriorQsoTotalCount = (uint)Math.Max(0, page.Total);
        }
        catch (StorageException)
        {
            // History is best-effort; never fail the lookup itself.
        }
    }

    private static QsoHistoryEntry QsoToHistoryEntry(QsoRecord qso)
    {
        var entry = new QsoHistoryEntry
        {
            LocalId = qso.LocalId,
            UtcTimestamp = qso.UtcTimestamp,
            Band = qso.Band,
            Mode = qso.Mode,
        };

        if (!string.IsNullOrEmpty(qso.Submode))
        {
            entry.Submode = qso.Submode;
        }

        if (qso.HasFrequencyHz)
        {
            entry.FrequencyHz = qso.FrequencyHz;
        }

        if (qso.HasFrequencyRxHz)
        {
            entry.FrequencyRxHz = qso.FrequencyRxHz;
        }

        if (!string.IsNullOrEmpty(qso.ContestId))
        {
            entry.ContestId = qso.ContestId;
        }

        return entry;
    }

    private sealed record CacheEntry(CallsignRecord? Record, DateTimeOffset CachedAt);
}
