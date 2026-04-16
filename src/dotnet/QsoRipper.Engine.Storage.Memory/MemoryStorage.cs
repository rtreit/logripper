using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;

namespace QsoRipper.Engine.Storage.Memory;

/// <summary>
/// In-memory implementation of <see cref="IEngineStorage"/>.
/// Thread-safe via <see cref="Lock"/>. QSO records are cloned on read/write
/// to prevent callers from mutating stored data.
/// </summary>
public sealed class MemoryStorage : IEngineStorage, ILogbookStore, ILookupSnapshotStore
{
    private readonly Lock _lock = new();
    private readonly SortedDictionary<string, QsoRecord> _qsos = new(StringComparer.Ordinal);
    private readonly Dictionary<string, LookupSnapshot> _lookupSnapshots = new(StringComparer.OrdinalIgnoreCase);
    private SyncMetadata _syncMetadata = new();

    /// <inheritdoc />
    public string BackendName => "memory";

    /// <inheritdoc />
    public ILogbookStore Logbook => this;

    /// <inheritdoc />
    public ILookupSnapshotStore LookupSnapshots => this;

    // ──────────────────────────────────────────────
    //  ILogbookStore
    // ──────────────────────────────────────────────

    /// <inheritdoc />
    public ValueTask InsertQsoAsync(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        lock (_lock)
        {
            if (!_qsos.TryAdd(qso.LocalId, qso.Clone()))
            {
                throw StorageException.Duplicate("QsoRecord", qso.LocalId);
            }
        }

        return default;
    }

    /// <inheritdoc />
    public ValueTask<bool> UpdateQsoAsync(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        lock (_lock)
        {
            if (!_qsos.ContainsKey(qso.LocalId))
            {
                return new ValueTask<bool>(false);
            }

            _qsos[qso.LocalId] = qso.Clone();
            return new ValueTask<bool>(true);
        }
    }

    /// <inheritdoc />
    public ValueTask<bool> DeleteQsoAsync(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_lock)
        {
            return new ValueTask<bool>(_qsos.Remove(localId.Trim()));
        }
    }

    /// <inheritdoc />
    public ValueTask<QsoRecord?> GetQsoAsync(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_lock)
        {
            if (_qsos.TryGetValue(localId.Trim(), out var stored))
            {
                return new ValueTask<QsoRecord?>(stored.Clone());
            }

            return new ValueTask<QsoRecord?>((QsoRecord?)null);
        }
    }

    /// <inheritdoc />
    public ValueTask<IReadOnlyList<QsoRecord>> ListQsosAsync(QsoListQuery query)
    {
        ArgumentNullException.ThrowIfNull(query);

        lock (_lock)
        {
            IEnumerable<QsoRecord> items = _qsos.Values;

            if (query.After is { } after)
            {
                items = items.Where(q => ToOffset(q.UtcTimestamp) > after);
            }

            if (query.Before is { } before)
            {
                items = items.Where(q => ToOffset(q.UtcTimestamp) < before);
            }

            if (!string.IsNullOrWhiteSpace(query.CallsignFilter))
            {
                var filter = query.CallsignFilter.Trim();
                items = items.Where(q =>
                    q.WorkedCallsign.Contains(filter, StringComparison.OrdinalIgnoreCase));
            }

            if (query.BandFilter is { } band)
            {
                items = items.Where(q => q.Band == band);
            }

            if (query.ModeFilter is { } mode)
            {
                items = items.Where(q => q.Mode == mode);
            }

            if (!string.IsNullOrWhiteSpace(query.ContestId))
            {
                items = items.Where(q =>
                    string.Equals(q.ContestId, query.ContestId, StringComparison.OrdinalIgnoreCase));
            }

            items = query.Sort == QsoSortOrder.OldestFirst
                ? items.OrderBy(q => ToOffset(q.UtcTimestamp)).ThenBy(q => q.LocalId, StringComparer.Ordinal)
                : items.OrderByDescending(q => ToOffset(q.UtcTimestamp)).ThenByDescending(q => q.LocalId, StringComparer.Ordinal);

            if (query.Offset > 0)
            {
                items = items.Skip(query.Offset);
            }

            if (query.Limit is { } limit)
            {
                items = items.Take(limit);
            }

            IReadOnlyList<QsoRecord> result = items.Select(q => q.Clone()).ToArray();
            return new ValueTask<IReadOnlyList<QsoRecord>>(result);
        }
    }

    /// <inheritdoc />
    public ValueTask<LogbookCounts> GetCountsAsync()
    {
        lock (_lock)
        {
            var total = _qsos.Count;
            var pending = _qsos.Values.Count(q => q.SyncStatus != SyncStatus.Synced);
            return new ValueTask<LogbookCounts>(new LogbookCounts(total, pending));
        }
    }

    /// <inheritdoc />
    public ValueTask<SyncMetadata> GetSyncMetadataAsync()
    {
        lock (_lock)
        {
            return new ValueTask<SyncMetadata>(_syncMetadata);
        }
    }

    /// <inheritdoc />
    public ValueTask UpsertSyncMetadataAsync(SyncMetadata metadata)
    {
        ArgumentNullException.ThrowIfNull(metadata);

        lock (_lock)
        {
            _syncMetadata = metadata;
        }

        return default;
    }

    // ──────────────────────────────────────────────
    //  ILookupSnapshotStore
    // ──────────────────────────────────────────────

    /// <inheritdoc />
    public ValueTask<LookupSnapshot?> GetAsync(string callsign)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        var key = NormalizeCallsign(callsign);
        lock (_lock)
        {
            if (_lookupSnapshots.TryGetValue(key, out var snapshot))
            {
                return new ValueTask<LookupSnapshot?>(snapshot with { Result = snapshot.Result.Clone() });
            }

            return new ValueTask<LookupSnapshot?>((LookupSnapshot?)null);
        }
    }

    /// <inheritdoc />
    public ValueTask UpsertAsync(LookupSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);

        var key = NormalizeCallsign(snapshot.Callsign);
        lock (_lock)
        {
            _lookupSnapshots[key] = snapshot with { Result = snapshot.Result.Clone() };
        }

        return default;
    }

    /// <inheritdoc />
    public ValueTask<bool> DeleteAsync(string callsign)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        var key = NormalizeCallsign(callsign);
        lock (_lock)
        {
            return new ValueTask<bool>(_lookupSnapshots.Remove(key));
        }
    }

    // ──────────────────────────────────────────────
    //  Helpers
    // ──────────────────────────────────────────────

    private static string NormalizeCallsign(string callsign) => callsign.Trim().ToUpperInvariant();

    private static DateTimeOffset ToOffset(Timestamp? ts)
        => ts is null ? DateTimeOffset.MinValue : ts.ToDateTimeOffset();
}
