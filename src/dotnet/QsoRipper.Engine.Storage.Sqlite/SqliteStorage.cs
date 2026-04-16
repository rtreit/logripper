using System.Globalization;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using Microsoft.Data.Sqlite;
using QsoRipper.Domain;

namespace QsoRipper.Engine.Storage.Sqlite;

/// <summary>
/// SQLite-backed implementation of <see cref="IEngineStorage"/>.
/// Thread-safe via <see cref="Lock"/> around the single connection (same pattern as the Rust Mutex&lt;Connection&gt;).
/// QSO records and lookup results are stored as protobuf binary blobs with indexed columns for SQL filtering.
/// </summary>
public sealed class SqliteStorage : IEngineStorage, ILogbookStore, ILookupSnapshotStore, IDisposable
{
    private const string MigrationSql =
        """
        CREATE TABLE IF NOT EXISTS qsos (
            local_id TEXT PRIMARY KEY NOT NULL,
            qrz_logid TEXT,
            qrz_bookid TEXT,
            station_callsign TEXT NOT NULL,
            worked_callsign TEXT NOT NULL,
            utc_timestamp_ms INTEGER,
            band INTEGER NOT NULL,
            mode INTEGER NOT NULL,
            contest_id TEXT,
            created_at_ms INTEGER,
            updated_at_ms INTEGER,
            sync_status INTEGER NOT NULL,
            record BLOB NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_qsos_station_callsign ON qsos (station_callsign);
        CREATE INDEX IF NOT EXISTS idx_qsos_worked_callsign ON qsos (worked_callsign);
        CREATE INDEX IF NOT EXISTS idx_qsos_utc_timestamp_ms ON qsos (utc_timestamp_ms);
        CREATE INDEX IF NOT EXISTS idx_qsos_band ON qsos (band);
        CREATE INDEX IF NOT EXISTS idx_qsos_mode ON qsos (mode);
        CREATE INDEX IF NOT EXISTS idx_qsos_contest_id ON qsos (contest_id);
        CREATE INDEX IF NOT EXISTS idx_qsos_sync_status ON qsos (sync_status);

        CREATE TABLE IF NOT EXISTS sync_metadata (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            qrz_qso_count INTEGER NOT NULL DEFAULT 0,
            last_sync_ms INTEGER,
            qrz_logbook_owner TEXT
        );

        INSERT OR IGNORE INTO sync_metadata (id, qrz_qso_count) VALUES (1, 0);

        CREATE TABLE IF NOT EXISTS lookup_snapshots (
            callsign TEXT PRIMARY KEY NOT NULL,
            result BLOB NOT NULL,
            stored_at_ms INTEGER NOT NULL,
            expires_at_ms INTEGER
        );
        """;

    private readonly Lock _lock = new();
    private readonly SqliteConnection _connection;
    private bool _disposed;

    internal SqliteStorage(SqliteConnection connection)
    {
        _connection = connection;
    }

    /// <inheritdoc />
    public string BackendName => "sqlite";

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
            ThrowIfDisposed();
            try
            {
                using var cmd = _connection.CreateCommand();
                cmd.CommandText =
                    """
                    INSERT INTO qsos (local_id, qrz_logid, qrz_bookid, station_callsign, worked_callsign,
                        utc_timestamp_ms, band, mode, contest_id, created_at_ms, updated_at_ms, sync_status, record)
                    VALUES ($local_id, $qrz_logid, $qrz_bookid, $station_callsign, $worked_callsign,
                        $utc_timestamp_ms, $band, $mode, $contest_id, $created_at_ms, $updated_at_ms, $sync_status, $record)
                    """;
                BindQsoParameters(cmd, qso);
                cmd.ExecuteNonQuery();
            }
            catch (SqliteException ex) when (ex.SqliteErrorCode == 19) // SQLITE_CONSTRAINT
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
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText =
                """
                UPDATE qsos SET
                    qrz_logid = $qrz_logid,
                    qrz_bookid = $qrz_bookid,
                    station_callsign = $station_callsign,
                    worked_callsign = $worked_callsign,
                    utc_timestamp_ms = $utc_timestamp_ms,
                    band = $band,
                    mode = $mode,
                    contest_id = $contest_id,
                    created_at_ms = $created_at_ms,
                    updated_at_ms = $updated_at_ms,
                    sync_status = $sync_status,
                    record = $record
                WHERE local_id = $local_id
                """;
            BindQsoParameters(cmd, qso);
            var rows = cmd.ExecuteNonQuery();
            return new ValueTask<bool>(rows > 0);
        }
    }

    /// <inheritdoc />
    public ValueTask<bool> DeleteQsoAsync(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_lock)
        {
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText = "DELETE FROM qsos WHERE local_id = $local_id";
            cmd.Parameters.AddWithValue("$local_id", localId.Trim());
            var rows = cmd.ExecuteNonQuery();
            return new ValueTask<bool>(rows > 0);
        }
    }

    /// <inheritdoc />
    public ValueTask<QsoRecord?> GetQsoAsync(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_lock)
        {
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText = "SELECT record FROM qsos WHERE local_id = $local_id";
            cmd.Parameters.AddWithValue("$local_id", localId.Trim());
            using var reader = cmd.ExecuteReader();
            if (reader.Read())
            {
                var blob = (byte[])reader["record"];
                return new ValueTask<QsoRecord?>(QsoRecord.Parser.ParseFrom(blob));
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
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            var whereClauses = new List<string>();

            if (query.After is { } after)
            {
                whereClauses.Add("utc_timestamp_ms > $after");
                cmd.Parameters.AddWithValue("$after", after.ToUnixTimeMilliseconds());
            }

            if (query.Before is { } before)
            {
                whereClauses.Add("utc_timestamp_ms < $before");
                cmd.Parameters.AddWithValue("$before", before.ToUnixTimeMilliseconds());
            }

            if (!string.IsNullOrWhiteSpace(query.CallsignFilter))
            {
                whereClauses.Add("worked_callsign LIKE $callsign_filter");
                cmd.Parameters.AddWithValue("$callsign_filter", "%" + query.CallsignFilter.Trim() + "%");
            }

            if (query.BandFilter is { } band)
            {
                whereClauses.Add("band = $band");
                cmd.Parameters.AddWithValue("$band", (int)band);
            }

            if (query.ModeFilter is { } mode)
            {
                whereClauses.Add("mode = $mode");
                cmd.Parameters.AddWithValue("$mode", (int)mode);
            }

            if (!string.IsNullOrWhiteSpace(query.ContestId))
            {
                whereClauses.Add("contest_id = $contest_id COLLATE NOCASE");
                cmd.Parameters.AddWithValue("$contest_id", query.ContestId.Trim());
            }

            var orderDirection = query.Sort == QsoSortOrder.OldestFirst ? "ASC" : "DESC";
            var whereClause = whereClauses.Count > 0
                ? "WHERE " + string.Join(" AND ", whereClauses)
                : string.Empty;

            var sql = string.Create(
                CultureInfo.InvariantCulture,
                $"SELECT record FROM qsos {whereClause} ORDER BY utc_timestamp_ms {orderDirection}, local_id {orderDirection}");

            if (query.Limit is { } limit)
            {
                sql += string.Create(CultureInfo.InvariantCulture, $" LIMIT {limit}");
            }

            if (query.Offset > 0)
            {
                if (query.Limit is null)
                {
                    sql += " LIMIT -1";
                }

                sql += string.Create(CultureInfo.InvariantCulture, $" OFFSET {query.Offset}");
            }


#pragma warning disable CA2100 // SQL is built from controlled enum/int values, all user input is parameterized
            cmd.CommandText = sql;
#pragma warning restore CA2100
            using var reader = cmd.ExecuteReader();
            var results = new List<QsoRecord>();
            while (reader.Read())
            {
                var blob = (byte[])reader["record"];
                results.Add(QsoRecord.Parser.ParseFrom(blob));
            }

            IReadOnlyList<QsoRecord> readOnly = results;
            return new ValueTask<IReadOnlyList<QsoRecord>>(readOnly);
        }
    }

    /// <inheritdoc />
    public ValueTask<LogbookCounts> GetCountsAsync()
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            using var totalCmd = _connection.CreateCommand();
            totalCmd.CommandText = "SELECT COUNT(*) FROM qsos";
            var total = Convert.ToInt32(totalCmd.ExecuteScalar(), CultureInfo.InvariantCulture);

            using var pendingCmd = _connection.CreateCommand();
            pendingCmd.CommandText = "SELECT COUNT(*) FROM qsos WHERE sync_status != $synced";
            pendingCmd.Parameters.AddWithValue("$synced", (int)SyncStatus.Synced);
            var pending = Convert.ToInt32(pendingCmd.ExecuteScalar(), CultureInfo.InvariantCulture);

            return new ValueTask<LogbookCounts>(new LogbookCounts(total, pending));
        }
    }

    /// <inheritdoc />
    public ValueTask<SyncMetadata> GetSyncMetadataAsync()
    {
        lock (_lock)
        {
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText = "SELECT qrz_qso_count, last_sync_ms, qrz_logbook_owner FROM sync_metadata WHERE id = 1";
            using var reader = cmd.ExecuteReader();
            if (reader.Read())
            {
                var qrzQsoCount = reader.GetInt32(0);
                var lastSyncMs = reader.IsDBNull(1) ? (long?)null : reader.GetInt64(1);
                var owner = reader.IsDBNull(2) ? null : reader.GetString(2);

                return new ValueTask<SyncMetadata>(new SyncMetadata
                {
                    QrzQsoCount = qrzQsoCount,
                    LastSync = lastSyncMs is { } ms ? DateTimeOffset.FromUnixTimeMilliseconds(ms) : null,
                    QrzLogbookOwner = owner,
                });
            }

            return new ValueTask<SyncMetadata>(new SyncMetadata());
        }
    }

    /// <inheritdoc />
    public ValueTask UpsertSyncMetadataAsync(SyncMetadata metadata)
    {
        ArgumentNullException.ThrowIfNull(metadata);

        lock (_lock)
        {
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText =
                """
                INSERT INTO sync_metadata (id, qrz_qso_count, last_sync_ms, qrz_logbook_owner)
                VALUES (1, $qrz_qso_count, $last_sync_ms, $qrz_logbook_owner)
                ON CONFLICT(id) DO UPDATE SET
                    qrz_qso_count = excluded.qrz_qso_count,
                    last_sync_ms = excluded.last_sync_ms,
                    qrz_logbook_owner = excluded.qrz_logbook_owner
                """;
            cmd.Parameters.AddWithValue("$qrz_qso_count", metadata.QrzQsoCount);
            cmd.Parameters.AddWithValue("$last_sync_ms",
                metadata.LastSync is { } lastSync ? (object)lastSync.ToUnixTimeMilliseconds() : DBNull.Value);
            cmd.Parameters.AddWithValue("$qrz_logbook_owner",
                metadata.QrzLogbookOwner is { } owner ? (object)owner : DBNull.Value);
            cmd.ExecuteNonQuery();
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
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText = "SELECT callsign, result, stored_at_ms, expires_at_ms FROM lookup_snapshots WHERE callsign = $callsign";
            cmd.Parameters.AddWithValue("$callsign", key);
            using var reader = cmd.ExecuteReader();
            if (reader.Read())
            {
                return new ValueTask<LookupSnapshot?>(ReadLookupSnapshot(reader));
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
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText =
                """
                INSERT INTO lookup_snapshots (callsign, result, stored_at_ms, expires_at_ms)
                VALUES ($callsign, $result, $stored_at_ms, $expires_at_ms)
                ON CONFLICT(callsign) DO UPDATE SET
                    result = excluded.result,
                    stored_at_ms = excluded.stored_at_ms,
                    expires_at_ms = excluded.expires_at_ms
                """;
            cmd.Parameters.AddWithValue("$callsign", key);
            cmd.Parameters.AddWithValue("$result", snapshot.Result.ToByteArray());
            cmd.Parameters.AddWithValue("$stored_at_ms", snapshot.StoredAt.ToUnixTimeMilliseconds());
            cmd.Parameters.AddWithValue("$expires_at_ms",
                snapshot.ExpiresAt is { } expiresAt ? (object)expiresAt.ToUnixTimeMilliseconds() : DBNull.Value);
            cmd.ExecuteNonQuery();
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
            ThrowIfDisposed();
            using var cmd = _connection.CreateCommand();
            cmd.CommandText = "DELETE FROM lookup_snapshots WHERE callsign = $callsign";
            cmd.Parameters.AddWithValue("$callsign", key);
            var rows = cmd.ExecuteNonQuery();
            return new ValueTask<bool>(rows > 0);
        }
    }

    // ──────────────────────────────────────────────
    //  IDisposable
    // ──────────────────────────────────────────────

    public void Dispose()
    {
        lock (_lock)
        {
            if (!_disposed)
            {
                _disposed = true;
                _connection.Dispose();
            }
        }
    }

    // ──────────────────────────────────────────────
    //  Internal: initialization
    // ──────────────────────────────────────────────

    internal void RunMigrations()
    {
        lock (_lock)
        {
            using var cmd = _connection.CreateCommand();
            cmd.CommandText = MigrationSql;
            cmd.ExecuteNonQuery();
        }
    }

    internal void ConfigurePragmas(TimeSpan busyTimeout)
    {
        lock (_lock)
        {
            ExecutePragma("journal_mode", "WAL");
            ExecutePragma("foreign_keys", "ON");
            ExecutePragma("busy_timeout", ((int)busyTimeout.TotalMilliseconds).ToString(CultureInfo.InvariantCulture));
        }
    }

    // ──────────────────────────────────────────────
    //  Helpers
    // ──────────────────────────────────────────────

    private static string NormalizeCallsign(string callsign) => callsign.Trim().ToUpperInvariant();

    private static long? TimestampToMs(Timestamp? ts)
    {
        if (ts is null || (ts.Seconds == 0 && ts.Nanos == 0))
        {
            return null;
        }

        return (ts.Seconds * 1000) + (ts.Nanos / 1_000_000);
    }

    private static string? NullIfEmpty(string? value)
        => string.IsNullOrEmpty(value) ? null : value;

    private static void BindQsoParameters(SqliteCommand cmd, QsoRecord qso)
    {
        cmd.Parameters.AddWithValue("$local_id", qso.LocalId);
        cmd.Parameters.AddWithValue("$qrz_logid", (object?)NullIfEmpty(qso.QrzLogid) ?? DBNull.Value);
        cmd.Parameters.AddWithValue("$qrz_bookid", (object?)NullIfEmpty(qso.QrzBookid) ?? DBNull.Value);
        cmd.Parameters.AddWithValue("$station_callsign", qso.StationCallsign);
        cmd.Parameters.AddWithValue("$worked_callsign", qso.WorkedCallsign);
        cmd.Parameters.AddWithValue("$utc_timestamp_ms", NullableMsToDbValue(TimestampToMs(qso.UtcTimestamp)));
        cmd.Parameters.AddWithValue("$band", (int)qso.Band);
        cmd.Parameters.AddWithValue("$mode", (int)qso.Mode);
        cmd.Parameters.AddWithValue("$contest_id", (object?)NullIfEmpty(qso.ContestId) ?? DBNull.Value);
        cmd.Parameters.AddWithValue("$created_at_ms", NullableMsToDbValue(TimestampToMs(qso.CreatedAt)));
        cmd.Parameters.AddWithValue("$updated_at_ms", NullableMsToDbValue(TimestampToMs(qso.UpdatedAt)));
        cmd.Parameters.AddWithValue("$sync_status", (int)qso.SyncStatus);
        cmd.Parameters.AddWithValue("$record", qso.ToByteArray());
    }

    private static object NullableMsToDbValue(long? value)
        => value.HasValue ? value.Value : DBNull.Value;

    private static LookupSnapshot ReadLookupSnapshot(SqliteDataReader reader)
    {
        var storedCallsign = reader.GetString(0);
        var resultBlob = (byte[])reader["result"];
        var storedAtMs = reader.GetInt64(2);
        var expiresAtMs = reader.IsDBNull(3) ? (long?)null : reader.GetInt64(3);

        return new LookupSnapshot
        {
            Callsign = storedCallsign,
            Result = LookupResult.Parser.ParseFrom(resultBlob),
            StoredAt = DateTimeOffset.FromUnixTimeMilliseconds(storedAtMs),
            ExpiresAt = expiresAtMs is { } ms ? DateTimeOffset.FromUnixTimeMilliseconds(ms) : null,
        };
    }

    private void ExecutePragma(string pragma, string value)
    {
        using var cmd = _connection.CreateCommand();
#pragma warning disable CA2100 // Pragma names and values are compile-time constants from ConfigurePragmas
        cmd.CommandText = $"PRAGMA {pragma} = {value}";
#pragma warning restore CA2100
        cmd.ExecuteNonQuery();
    }

    private void ThrowIfDisposed()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
    }
}
