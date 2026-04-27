using Google.Protobuf.WellKnownTypes;
using Microsoft.Data.Sqlite;
using QsoRipper.Domain;
using QsoRipper.Engine.Storage;
using QsoRipper.Engine.Storage.Sqlite;

namespace QsoRipper.Engine.Storage.Sqlite.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class SqliteStorageTests : IDisposable
{
    private readonly SqliteStorage _storage;

    public SqliteStorageTests()
    {
        _storage = new SqliteStorageBuilder().InMemory().Build();
    }

    public void Dispose()
    {
        _storage.Dispose();
    }

    // ──────────────────────────────────────────────
    //  Backend metadata
    // ──────────────────────────────────────────────

    [Fact]
    public void BackendName_is_sqlite()
    {
        Assert.Equal("sqlite", _storage.BackendName);
    }

    [Fact]
    public void Logbook_and_LookupSnapshots_are_not_null()
    {
        Assert.NotNull(_storage.Logbook);
        Assert.NotNull(_storage.LookupSnapshots);
    }

    // ──────────────────────────────────────────────
    //  QSO CRUD
    // ──────────────────────────────────────────────

    [Fact]
    public async Task Insert_and_get_qso_round_trips()
    {
        var qso = MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z");
        await _storage.Logbook.InsertQsoAsync(qso);

        var loaded = await _storage.Logbook.GetQsoAsync("q1");

        Assert.NotNull(loaded);
        Assert.Equal("q1", loaded!.LocalId);
        Assert.Equal("W1AW", loaded.WorkedCallsign);
        Assert.Equal(Band._20M, loaded.Band);
        Assert.Equal(Mode.Ft8, loaded.Mode);
    }

    [Fact]
    public async Task Insert_stores_data_independently_of_original_object()
    {
        var qso = MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z");
        await _storage.Logbook.InsertQsoAsync(qso);

        // Mutate original — stored data should be unaffected because it's serialized to blob.
        qso.WorkedCallsign = "MUTATED";

        var loaded = await _storage.Logbook.GetQsoAsync("q1");
        Assert.Equal("W1AW", loaded!.WorkedCallsign);
    }

    [Fact]
    public async Task Get_returns_independent_copy()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));

        var loaded = await _storage.Logbook.GetQsoAsync("q1");
        loaded!.WorkedCallsign = "MUTATED";

        var loadedAgain = await _storage.Logbook.GetQsoAsync("q1");
        Assert.Equal("W1AW", loadedAgain!.WorkedCallsign);
    }

    [Fact]
    public async Task Insert_duplicate_throws_StorageException()
    {
        var qso = MakeQso("dup", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z");
        await _storage.Logbook.InsertQsoAsync(qso);

        var ex = await Assert.ThrowsAsync<StorageException>(
            () => _storage.Logbook.InsertQsoAsync(MakeQso("dup", "K7RND", Band._40M, Mode.Cw, "2026-01-16T00:00:00Z")).AsTask());

        Assert.Equal(StorageErrorKind.Duplicate, ex.Kind);
        Assert.Contains("dup", ex.Message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Insert_check_constraint_violation_throws_Backend_not_Duplicate()
    {
        // Use a file-based DB so we can add a CHECK constraint via a second connection.
        var dbPath = Path.Combine(Path.GetTempPath(), $"qsoripper-check-test-{Guid.NewGuid():N}.db");
        try
        {
            using var storage = new SqliteStorageBuilder().Path(dbPath).Build();

            // Add a CHECK constraint that rejects worked_callsign = 'BLOCKED'.
            using (var adminConn = new SqliteConnection($"Data Source={dbPath}"))
            {
                adminConn.Open();
                using var cmd = adminConn.CreateCommand();
                cmd.CommandText =
                    "CREATE TRIGGER check_worked_callsign BEFORE INSERT ON qsos " +
                    "BEGIN SELECT RAISE(ABORT, 'CHECK: worked_callsign blocked') " +
                    "WHERE NEW.worked_callsign = 'BLOCKED'; END;";
                cmd.ExecuteNonQuery();
            }

            var qso = MakeQso("chk1", "BLOCKED", Band._20M, Mode.Cw, "2026-01-15T12:00:00Z");
            var ex = await Assert.ThrowsAsync<StorageException>(
                () => storage.Logbook.InsertQsoAsync(qso).AsTask());

            Assert.Equal(StorageErrorKind.Backend, ex.Kind);
            Assert.Contains("Constraint violation", ex.Message, StringComparison.Ordinal);
        }
        finally
        {
            SqliteConnection.ClearAllPools();
            if (File.Exists(dbPath))
            {
                File.Delete(dbPath);
            }
        }
    }

    [Fact]
    public async Task Get_nonexistent_returns_null()
    {
        var result = await _storage.Logbook.GetQsoAsync("no-such-id");
        Assert.Null(result);
    }

    [Fact]
    public async Task Update_existing_qso_returns_true_and_persists()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));

        var updated = MakeQso("q1", "W1AW", Band._40M, Mode.Cw, "2026-01-15T12:00:00Z");
        var result = await _storage.Logbook.UpdateQsoAsync(updated);

        Assert.True(result);
        var loaded = await _storage.Logbook.GetQsoAsync("q1");
        Assert.Equal(Band._40M, loaded!.Band);
        Assert.Equal(Mode.Cw, loaded.Mode);
    }

    [Fact]
    public async Task Update_nonexistent_returns_false()
    {
        var result = await _storage.Logbook.UpdateQsoAsync(MakeQso("missing", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));
        Assert.False(result);
    }

    [Fact]
    public async Task Delete_existing_returns_true()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));

        var deleted = await _storage.Logbook.DeleteQsoAsync("q1");

        Assert.True(deleted);
        Assert.Null(await _storage.Logbook.GetQsoAsync("q1"));
    }

    [Fact]
    public async Task Delete_nonexistent_returns_false()
    {
        var result = await _storage.Logbook.DeleteQsoAsync("missing");
        Assert.False(result);
    }

    // ──────────────────────────────────────────────
    //  QSO round-trip preserves all proto fields
    // ──────────────────────────────────────────────

    [Fact]
    public async Task Insert_get_preserves_all_proto_fields()
    {
        var qso = new QsoRecord
        {
            LocalId = "full",
            StationCallsign = "K7RND",
            WorkedCallsign = "W1AW",
            Band = Band._20M,
            Mode = Mode.Ft8,
            UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-01-15T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture)),
            SyncStatus = SyncStatus.Synced,
            ContestId = "WWDX",
            Notes = "Test notes",
        };
        qso.ExtraFields.Add("MY_CUSTOM_FIELD", "custom_value");

        await _storage.Logbook.InsertQsoAsync(qso);
        var loaded = await _storage.Logbook.GetQsoAsync("full");

        Assert.NotNull(loaded);
        Assert.Equal("K7RND", loaded!.StationCallsign);
        Assert.Equal("WWDX", loaded.ContestId);
        Assert.Equal("Test notes", loaded.Notes);
        Assert.Equal("custom_value", loaded.ExtraFields["MY_CUSTOM_FIELD"]);
        Assert.Equal(SyncStatus.Synced, loaded.SyncStatus);
    }

    // ──────────────────────────────────────────────
    //  ListQsos — filtering
    // ──────────────────────────────────────────────

    [Fact]
    public async Task List_no_filters_returns_all()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery());

        Assert.Equal(3, result.Count);
    }

    [Fact]
    public async Task List_filters_by_after()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            After = DateTimeOffset.Parse("2026-01-15T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture),
        });

        // After is inclusive: q1 at exactly 2026-01-15T12:00:00Z is included.
        Assert.Equal(3, result.Count);
        Assert.All(result, q => Assert.True(q.UtcTimestamp.ToDateTimeOffset() >= DateTimeOffset.Parse("2026-01-15T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture)));
    }

    [Fact]
    public async Task List_filters_by_before()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            Before = DateTimeOffset.Parse("2026-01-16T00:00:00Z", System.Globalization.CultureInfo.InvariantCulture),
        });

        // Before is inclusive: q2 at exactly 2026-01-16T00:00:00Z is included.
        Assert.Equal(2, result.Count);
    }

    [Fact]
    public async Task List_filters_by_callsign_substring()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            CallsignFilter = "W1",
        });

        Assert.Equal(2, result.Count);
        Assert.All(result, q => Assert.Contains("W1", q.WorkedCallsign, StringComparison.OrdinalIgnoreCase));
    }

    [Fact]
    public async Task List_callsign_filter_is_case_insensitive()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery { CallsignFilter = "w1aw" });

        Assert.Single(result);
    }

    [Fact]
    public async Task List_callsign_filter_matches_station_or_worked()
    {
        var qso = MakeQso("q1", "DL1XYZ", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z");
        qso.StationCallsign = "K7RND";
        await _storage.Logbook.InsertQsoAsync(qso);

        var byStation = await _storage.Logbook.ListQsosAsync(new QsoListQuery { CallsignFilter = "K7RND" });
        var byWorked = await _storage.Logbook.ListQsosAsync(new QsoListQuery { CallsignFilter = "DL1" });

        Assert.Single(byStation);
        Assert.Single(byWorked);
    }

    [Fact]
    public async Task List_filters_by_band()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery { BandFilter = Band._40M });

        Assert.Single(result);
        Assert.Equal(Band._40M, result[0].Band);
    }

    [Fact]
    public async Task List_filters_by_mode()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery { ModeFilter = Mode.Cw });

        Assert.Single(result);
        Assert.Equal(Mode.Cw, result[0].Mode);
    }

    [Fact]
    public async Task List_filters_by_contest_id()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z", contestId: "WWDX"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("q2", "K7RND", Band._40M, Mode.Cw, "2026-01-16T00:00:00Z", contestId: "STATEQP"));

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery { ContestId = "WWDX" });

        Assert.Single(result);
        Assert.Equal("q1", result[0].LocalId);
    }

    [Fact]
    public async Task List_contest_filter_is_case_insensitive()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z", contestId: "WWDX"));

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery { ContestId = "wwdx" });

        Assert.Single(result);
    }

    [Fact]
    public async Task List_combined_filters()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            BandFilter = Band._20M,
            ModeFilter = Mode.Ft8,
        });

        Assert.Single(result);
        Assert.Equal("q1", result[0].LocalId);
    }

    // ──────────────────────────────────────────────
    //  ListQsos — sorting
    // ──────────────────────────────────────────────

    [Fact]
    public async Task List_default_sort_is_newest_first()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery());

        Assert.Equal("q3", result[0].LocalId);
        Assert.Equal("q2", result[1].LocalId);
        Assert.Equal("q1", result[2].LocalId);
    }

    [Fact]
    public async Task List_oldest_first_sort()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery { Sort = QsoSortOrder.OldestFirst });

        Assert.Equal("q1", result[0].LocalId);
        Assert.Equal("q2", result[1].LocalId);
        Assert.Equal("q3", result[2].LocalId);
    }

    // ──────────────────────────────────────────────
    //  ListQsos — pagination
    // ──────────────────────────────────────────────

    [Fact]
    public async Task List_with_limit()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            Sort = QsoSortOrder.OldestFirst,
            Limit = 2,
        });

        Assert.Equal(2, result.Count);
        Assert.Equal("q1", result[0].LocalId);
        Assert.Equal("q2", result[1].LocalId);
    }

    [Fact]
    public async Task List_with_offset()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            Sort = QsoSortOrder.OldestFirst,
            Offset = 1,
        });

        Assert.Equal(2, result.Count);
        Assert.Equal("q2", result[0].LocalId);
        Assert.Equal("q3", result[1].LocalId);
    }

    [Fact]
    public async Task List_with_offset_and_limit()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            Sort = QsoSortOrder.OldestFirst,
            Offset = 1,
            Limit = 1,
        });

        Assert.Single(result);
        Assert.Equal("q2", result[0].LocalId);
    }

    // ──────────────────────────────────────────────
    //  GetCounts
    // ──────────────────────────────────────────────

    [Fact]
    public async Task GetCounts_empty_store()
    {
        var counts = await _storage.Logbook.GetCountsAsync();

        Assert.Equal(0, counts.LocalQsoCount);
        Assert.Equal(0, counts.PendingUploadCount);
    }

    [Fact]
    public async Task GetCounts_reflects_sync_status()
    {
        var synced = MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z");
        synced.SyncStatus = SyncStatus.Synced;
        await _storage.Logbook.InsertQsoAsync(synced);

        var localOnly = MakeQso("q2", "K7RND", Band._40M, Mode.Cw, "2026-01-16T00:00:00Z");
        localOnly.SyncStatus = SyncStatus.LocalOnly;
        await _storage.Logbook.InsertQsoAsync(localOnly);

        var modified = MakeQso("q3", "N0CALL", Band._10M, Mode.Ssb, "2026-01-17T00:00:00Z");
        modified.SyncStatus = SyncStatus.Modified;
        await _storage.Logbook.InsertQsoAsync(modified);

        var counts = await _storage.Logbook.GetCountsAsync();

        Assert.Equal(3, counts.LocalQsoCount);
        Assert.Equal(2, counts.PendingUploadCount); // LocalOnly + Modified
    }

    // ──────────────────────────────────────────────
    //  SyncMetadata
    // ──────────────────────────────────────────────

    [Fact]
    public async Task SyncMetadata_defaults_to_empty()
    {
        var meta = await _storage.Logbook.GetSyncMetadataAsync();

        Assert.Null(meta.LastSync);
        Assert.Equal(0, meta.QrzQsoCount);
        Assert.Null(meta.QrzLogbookOwner);
    }

    [Fact]
    public async Task SyncMetadata_upsert_and_get_round_trips()
    {
        var now = DateTimeOffset.UtcNow;
        await _storage.Logbook.UpsertSyncMetadataAsync(new SyncMetadata
        {
            QrzQsoCount = 42,
            LastSync = now,
            QrzLogbookOwner = "K7RND",
        });

        var meta = await _storage.Logbook.GetSyncMetadataAsync();

        Assert.Equal(42, meta.QrzQsoCount);
        // Millisecond-level precision comparison (SQLite stores ms, not ticks)
        Assert.NotNull(meta.LastSync);
        Assert.Equal(now.ToUnixTimeMilliseconds(), meta.LastSync!.Value.ToUnixTimeMilliseconds());
        Assert.Equal("K7RND", meta.QrzLogbookOwner);
    }

    [Fact]
    public async Task SyncMetadata_upsert_replaces_previous()
    {
        await _storage.Logbook.UpsertSyncMetadataAsync(new SyncMetadata { QrzQsoCount = 1 });
        await _storage.Logbook.UpsertSyncMetadataAsync(new SyncMetadata { QrzQsoCount = 99, LastSync = DateTimeOffset.UtcNow });

        var meta = await _storage.Logbook.GetSyncMetadataAsync();

        Assert.Equal(99, meta.QrzQsoCount);
    }

    // ──────────────────────────────────────────────
    //  LookupSnapshot CRUD
    // ──────────────────────────────────────────────

    [Fact]
    public async Task Lookup_get_nonexistent_returns_null()
    {
        var result = await _storage.LookupSnapshots.GetAsync("W1AW");
        Assert.Null(result);
    }

    [Fact]
    public async Task Lookup_upsert_and_get_round_trips()
    {
        var snapshot = MakeLookupSnapshot("W1AW");
        await _storage.LookupSnapshots.UpsertAsync(snapshot);

        var loaded = await _storage.LookupSnapshots.GetAsync("W1AW");

        Assert.NotNull(loaded);
        Assert.Equal("W1AW", loaded!.Callsign);
        Assert.Equal(LookupState.Found, loaded.Result.State);
    }

    [Fact]
    public async Task Lookup_callsign_normalization_is_case_insensitive()
    {
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("W1AW"));

        var loaded = await _storage.LookupSnapshots.GetAsync("w1aw");

        Assert.NotNull(loaded);
    }

    [Fact]
    public async Task Lookup_callsign_normalization_trims_whitespace()
    {
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("W1AW"));

        var loaded = await _storage.LookupSnapshots.GetAsync("  W1AW  ");

        Assert.NotNull(loaded);
    }

    [Fact]
    public async Task Lookup_upsert_replaces_existing()
    {
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("W1AW"));

        var updated = new LookupSnapshot
        {
            Callsign = "W1AW",
            Result = new LookupResult { State = LookupState.NotFound, QueriedCallsign = "W1AW" },
            StoredAt = DateTimeOffset.UtcNow,
        };
        await _storage.LookupSnapshots.UpsertAsync(updated);

        var loaded = await _storage.LookupSnapshots.GetAsync("W1AW");
        Assert.Equal(LookupState.NotFound, loaded!.Result.State);
    }

    [Fact]
    public async Task Lookup_delete_existing_returns_true()
    {
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("W1AW"));

        var deleted = await _storage.LookupSnapshots.DeleteAsync("W1AW");

        Assert.True(deleted);
        Assert.Null(await _storage.LookupSnapshots.GetAsync("W1AW"));
    }

    [Fact]
    public async Task Lookup_delete_nonexistent_returns_false()
    {
        var result = await _storage.LookupSnapshots.DeleteAsync("NOTHING");
        Assert.False(result);
    }

    [Fact]
    public async Task Lookup_get_returns_independent_copy()
    {
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("W1AW"));

        var loaded = await _storage.LookupSnapshots.GetAsync("W1AW");
        loaded!.Result.QueriedCallsign = "MUTATED";

        var loadedAgain = await _storage.LookupSnapshots.GetAsync("W1AW");
        Assert.Equal("W1AW", loadedAgain!.Result.QueriedCallsign);
    }

    [Fact]
    public async Task Lookup_snapshot_preserves_expires_at()
    {
        var expiresAt = DateTimeOffset.UtcNow.AddHours(1);
        var snapshot = new LookupSnapshot
        {
            Callsign = "W1AW",
            Result = new LookupResult { State = LookupState.Found, QueriedCallsign = "W1AW" },
            StoredAt = DateTimeOffset.UtcNow,
            ExpiresAt = expiresAt,
        };
        await _storage.LookupSnapshots.UpsertAsync(snapshot);

        var loaded = await _storage.LookupSnapshots.GetAsync("W1AW");

        Assert.NotNull(loaded!.ExpiresAt);
        Assert.Equal(expiresAt.ToUnixTimeMilliseconds(), loaded.ExpiresAt!.Value.ToUnixTimeMilliseconds());
    }

    // ──────────────────────────────────────────────
    //  Builder
    // ──────────────────────────────────────────────

    [Fact]
    public void Builder_creates_file_based_storage()
    {
        var tempDir = Path.Combine(Path.GetTempPath(), "qsoripper-test-" + Guid.NewGuid().ToString("N"));
        var dbPath = Path.Combine(tempDir, "sub", "test.db");
        try
        {
            using (var storage = new SqliteStorageBuilder().Path(dbPath).Build())
            {
                Assert.Equal("sqlite", storage.BackendName);
                Assert.True(File.Exists(dbPath), $"Database file should exist at {dbPath}");
            }

            // Force SQLite to release WAL/SHM files on Windows by clearing the connection pool.
            Microsoft.Data.Sqlite.SqliteConnection.ClearAllPools();
        }
        finally
        {
            if (Directory.Exists(tempDir))
            {
                Directory.Delete(tempDir, true);
            }
        }
    }

    [Fact]
    public async Task Data_persists_across_operations()
    {
        // Insert, then verify data is still there after other operations.
        await _storage.Logbook.InsertQsoAsync(MakeQso("persist1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("K7RND"));
        await _storage.Logbook.UpsertSyncMetadataAsync(new SyncMetadata { QrzQsoCount = 10 });

        // Perform unrelated operations.
        await _storage.Logbook.InsertQsoAsync(MakeQso("persist2", "N0CALL", Band._10M, Mode.Ssb, "2026-01-16T00:00:00Z"));
        await _storage.Logbook.DeleteQsoAsync("persist2");

        // Verify original data still present.
        Assert.NotNull(await _storage.Logbook.GetQsoAsync("persist1"));
        Assert.NotNull(await _storage.LookupSnapshots.GetAsync("K7RND"));
        var meta = await _storage.Logbook.GetSyncMetadataAsync();
        Assert.Equal(10, meta.QrzQsoCount);
    }

    // ──────────────────────────────────────────────
    //  Helpers
    // ──────────────────────────────────────────────

    private async Task InsertThreeQsos()
    {
        // q1: 2026-01-15T12:00Z, 20m FT8
        await _storage.Logbook.InsertQsoAsync(MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z"));
        // q2: 2026-01-16T00:00Z, 40m CW
        await _storage.Logbook.InsertQsoAsync(MakeQso("q2", "W1NEW", Band._40M, Mode.Cw, "2026-01-16T00:00:00Z"));
        // q3: 2026-01-17T00:00Z, 10m SSB
        await _storage.Logbook.InsertQsoAsync(MakeQso("q3", "K7RND", Band._10M, Mode.Ssb, "2026-01-17T00:00:00Z"));
    }

    private static QsoRecord MakeQso(
        string localId,
        string workedCallsign,
        Band band,
        Mode mode,
        string utcTimestamp,
        SyncStatus syncStatus = SyncStatus.LocalOnly,
        string? contestId = null)
    {
        var qso = new QsoRecord
        {
            LocalId = localId,
            WorkedCallsign = workedCallsign,
            Band = band,
            Mode = mode,
            UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse(utcTimestamp, System.Globalization.CultureInfo.InvariantCulture)),
            SyncStatus = syncStatus,
        };

        if (contestId is not null)
        {
            qso.ContestId = contestId;
        }

        return qso;
    }

    private static LookupSnapshot MakeLookupSnapshot(string callsign)
    {
        return new LookupSnapshot
        {
            Callsign = callsign,
            Result = new LookupResult
            {
                State = LookupState.Found,
                QueriedCallsign = callsign,
                Record = new CallsignRecord { Callsign = callsign },
            },
            StoredAt = DateTimeOffset.UtcNow,
        };
    }

    // ──────────────────────────────────────────────
    //  Soft-delete & restore
    // ──────────────────────────────────────────────

    [Fact]
    public async Task SoftDelete_marks_row_and_keeps_it_persisted()
    {
        var qso = MakeQso("soft1", "W1AW", Band._20M, Mode.Ft8, "2026-02-01T00:00:00Z", SyncStatus.Synced);
        qso.QrzLogid = "remote-1";
        await _storage.Logbook.InsertQsoAsync(qso);

        var deletedAt = DateTimeOffset.Parse("2026-02-02T00:00:00Z", System.Globalization.CultureInfo.InvariantCulture);
        var ok = await _storage.Logbook.SoftDeleteQsoAsync("soft1", deletedAt, pendingRemoteDelete: true);
        Assert.True(ok);

        var fetched = await _storage.Logbook.GetQsoAsync("soft1");
        Assert.NotNull(fetched);
        Assert.NotNull(fetched!.DeletedAt);
        Assert.Equal(deletedAt.ToUnixTimeSeconds(), fetched.DeletedAt.Seconds);
        Assert.True(fetched.PendingRemoteDelete);
    }

    [Fact]
    public async Task ListQsos_default_filter_excludes_soft_deleted()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("a", "W1AW", Band._20M, Mode.Ft8, "2026-02-01T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("b", "W1NEW", Band._20M, Mode.Ft8, "2026-02-02T00:00:00Z"));
        await _storage.Logbook.SoftDeleteQsoAsync("b", DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var active = await _storage.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(active);
        Assert.Equal("a", active[0].LocalId);

        var deletedOnly = await _storage.Logbook.ListQsosAsync(new QsoListQuery { DeletedFilter = DeletedRecordsFilter.DeletedOnly });
        Assert.Single(deletedOnly);
        Assert.Equal("b", deletedOnly[0].LocalId);

        var all = await _storage.Logbook.ListQsosAsync(new QsoListQuery { DeletedFilter = DeletedRecordsFilter.All });
        Assert.Equal(2, all.Count);
    }

    [Fact]
    public async Task GetCounts_excludes_soft_deleted_rows()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("c1", "W1AW", Band._20M, Mode.Ft8, "2026-02-01T00:00:00Z", SyncStatus.LocalOnly));
        await _storage.Logbook.InsertQsoAsync(MakeQso("c2", "W1NEW", Band._20M, Mode.Ft8, "2026-02-02T00:00:00Z", SyncStatus.LocalOnly));
        await _storage.Logbook.SoftDeleteQsoAsync("c2", DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var counts = await _storage.Logbook.GetCountsAsync();
        Assert.Equal(1, counts.LocalQsoCount);
        Assert.Equal(1, counts.PendingUploadCount);
    }

    [Fact]
    public async Task Restore_clears_tombstone_and_pending_flag()
    {
        var qso = MakeQso("r1", "W1AW", Band._20M, Mode.Ft8, "2026-03-01T00:00:00Z");
        await _storage.Logbook.InsertQsoAsync(qso);
        await _storage.Logbook.SoftDeleteQsoAsync("r1", DateTimeOffset.UtcNow, pendingRemoteDelete: true);

        var ok = await _storage.Logbook.RestoreQsoAsync("r1");
        Assert.True(ok);

        var fetched = await _storage.Logbook.GetQsoAsync("r1");
        Assert.NotNull(fetched);
        Assert.Null(fetched!.DeletedAt);
        Assert.False(fetched.PendingRemoteDelete);
    }

    [Fact]
    public async Task SoftDelete_returns_false_for_missing_row()
    {
        var ok = await _storage.Logbook.SoftDeleteQsoAsync("missing", DateTimeOffset.UtcNow, pendingRemoteDelete: false);
        Assert.False(ok);
    }

    [Fact]
    public async Task Restore_returns_false_for_missing_row()
    {
        var ok = await _storage.Logbook.RestoreQsoAsync("missing");
        Assert.False(ok);
    }

    // ──────────────────────────────────────────────
    //  Purge deleted QSOs
    // ──────────────────────────────────────────────

    [Fact]
    public async Task Purge_removes_all_soft_deleted_rows()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("p1", "W1AW", Band._20M, Mode.Ft8, "2026-04-01T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("p2", "W1NEW", Band._40M, Mode.Cw, "2026-04-02T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("p3", "K7RND", Band._10M, Mode.Ssb, "2026-04-03T00:00:00Z"));

        await _storage.Logbook.SoftDeleteQsoAsync("p1", DateTimeOffset.UtcNow, pendingRemoteDelete: false);
        await _storage.Logbook.SoftDeleteQsoAsync("p2", DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var purged = await _storage.Logbook.PurgeDeletedQsosAsync(null, null);
        Assert.Equal(2, purged);

        Assert.Null(await _storage.Logbook.GetQsoAsync("p1"));
        Assert.Null(await _storage.Logbook.GetQsoAsync("p2"));
        Assert.NotNull(await _storage.Logbook.GetQsoAsync("p3"));
    }

    [Fact]
    public async Task Purge_filters_by_local_ids()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("f1", "W1AW", Band._20M, Mode.Ft8, "2026-04-01T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("f2", "W1NEW", Band._40M, Mode.Cw, "2026-04-02T00:00:00Z"));

        await _storage.Logbook.SoftDeleteQsoAsync("f1", DateTimeOffset.UtcNow, pendingRemoteDelete: false);
        await _storage.Logbook.SoftDeleteQsoAsync("f2", DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var purged = await _storage.Logbook.PurgeDeletedQsosAsync(["f1"], null);
        Assert.Equal(1, purged);

        Assert.Null(await _storage.Logbook.GetQsoAsync("f1"));
        Assert.NotNull(await _storage.Logbook.GetQsoAsync("f2"));
    }

    [Fact]
    public async Task Purge_filters_by_older_than()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("t1", "W1AW", Band._20M, Mode.Ft8, "2026-04-01T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("t2", "W1NEW", Band._40M, Mode.Cw, "2026-04-02T00:00:00Z"));

        var earlyDelete = DateTimeOffset.Parse("2026-05-01T00:00:00Z", System.Globalization.CultureInfo.InvariantCulture);
        var lateDelete = DateTimeOffset.Parse("2026-06-01T00:00:00Z", System.Globalization.CultureInfo.InvariantCulture);

        await _storage.Logbook.SoftDeleteQsoAsync("t1", earlyDelete, pendingRemoteDelete: false);
        await _storage.Logbook.SoftDeleteQsoAsync("t2", lateDelete, pendingRemoteDelete: false);

        var cutoff = DateTimeOffset.Parse("2026-05-15T00:00:00Z", System.Globalization.CultureInfo.InvariantCulture);
        var purged = await _storage.Logbook.PurgeDeletedQsosAsync(null, cutoff);
        Assert.Equal(1, purged);

        Assert.Null(await _storage.Logbook.GetQsoAsync("t1"));
        Assert.NotNull(await _storage.Logbook.GetQsoAsync("t2"));
    }

    [Fact]
    public async Task Purge_returns_zero_when_nothing_to_purge()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("n1", "W1AW", Band._20M, Mode.Ft8, "2026-04-01T00:00:00Z"));
        var purged = await _storage.Logbook.PurgeDeletedQsosAsync(null, null);
        Assert.Equal(0, purged);
    }

    [Fact]
    public async Task ListQsoHistory_exact_match_excludes_soft_deleted_and_caps_entries()
    {
        await _storage.Logbook.InsertQsoAsync(MakeQso("a", "K7ABC", Band._20M, Mode.Ssb, "2026-01-01T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("b", "K7ABC", Band._40M, Mode.Cw, "2026-02-01T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("c", "K7ABCD", Band._20M, Mode.Ssb, "2026-02-15T00:00:00Z"));
        await _storage.Logbook.InsertQsoAsync(MakeQso("d", "k7abc", Band._15M, Mode.Ft8, "2026-03-01T00:00:00Z"));
        await _storage.Logbook.SoftDeleteQsoAsync("b", DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var page = await _storage.Logbook.ListQsoHistoryAsync("k7abc", limit: 10);
        Assert.Equal(2, page.Total);
        Assert.Equal(2, page.Entries.Count);
        Assert.Equal("d", page.Entries[0].LocalId);
        Assert.Equal("a", page.Entries[1].LocalId);

        var limited = await _storage.Logbook.ListQsoHistoryAsync("K7ABC", limit: 1);
        Assert.Equal(2, limited.Total);
        Assert.Single(limited.Entries);
        Assert.Equal("d", limited.Entries[0].LocalId);

        var zero = await _storage.Logbook.ListQsoHistoryAsync("K7ABC", limit: 0);
        Assert.Equal(2, zero.Total);
        Assert.Empty(zero.Entries);

        var none = await _storage.Logbook.ListQsoHistoryAsync("NEVER", limit: 5);
        Assert.Equal(0, none.Total);
        Assert.Empty(none.Entries);
    }

    [Fact]
    public async Task Migration_idempotent_when_storage_reopens_existing_file()
    {
        var path = Path.Combine(Path.GetTempPath(), $"qsoripper-soft-delete-{Guid.NewGuid():N}.db");
        try
        {
            using (var first = new SqliteStorageBuilder().Path(path).Build())
            {
                var qso = MakeQso("p1", "W1AW", Band._20M, Mode.Ft8, "2026-04-01T00:00:00Z");
                await first.Logbook.InsertQsoAsync(qso);
                await first.Logbook.SoftDeleteQsoAsync("p1", DateTimeOffset.UtcNow, pendingRemoteDelete: false);
            }

            using (var second = new SqliteStorageBuilder().Path(path).Build())
            {
                // Reopening must not throw and must preserve the soft-delete state.
                var fetched = await second.Logbook.GetQsoAsync("p1");
                Assert.NotNull(fetched);
                Assert.NotNull(fetched!.DeletedAt);
            }
        }
        finally
        {
            Microsoft.Data.Sqlite.SqliteConnection.ClearAllPools();
            if (File.Exists(path))
            {
                try
                {
                    File.Delete(path);
                }
                catch (IOException)
                {
                }
            }
        }
    }
    [Fact]
    public async Task Migration_opens_legacy_pre_soft_delete_database_without_crash()
    {
        // Regression for #348: opening a database created before PR #289 must
        // not throw "no such column: deleted_at_ms". The bug was that the
        // bootstrap MigrationSql block created idx_qsos_deleted_at_ms before
        // ApplySoftDeleteMigration() had a chance to ALTER TABLE the column
        // in. On a fresh DB it worked because CREATE TABLE included the
        // column; on an upgraded DB the CREATE TABLE IF NOT EXISTS no-oped
        // and the CREATE INDEX exploded.
        var path = Path.Combine(Path.GetTempPath(), $"qsoripper-legacy-{Guid.NewGuid():N}.db");
        try
        {
            // Hand-build the pre-#289 schema: no deleted_at_ms, no
            // pending_remote_delete columns, no soft-delete index.
            using (var raw = new SqliteConnection($"Data Source={path}"))
            {
                raw.Open();
                using var cmd = raw.CreateCommand();
                cmd.CommandText = """
                    CREATE TABLE qsos (
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
                    """;
                cmd.ExecuteNonQuery();
            }
            SqliteConnection.ClearAllPools();

            // Opening must succeed — the migration has to detect the missing
            // columns, ALTER TABLE them in, then create the soft-delete index.
            using (var storage = new SqliteStorageBuilder().Path(path).Build())
            {
                // Soft-deleting must work end-to-end against the migrated DB.
                var qso = MakeQso("legacy-1", "W1AW", Band._20M, Mode.Ft8, "2026-04-01T00:00:00Z");
                qso.StationCallsign = "K7RND";
                await storage.Logbook.InsertQsoAsync(qso);
                await storage.Logbook.SoftDeleteQsoAsync("legacy-1", DateTimeOffset.UtcNow, pendingRemoteDelete: false);

                var fetched = await storage.Logbook.GetQsoAsync("legacy-1");
                Assert.NotNull(fetched);
                Assert.NotNull(fetched!.DeletedAt);
            }

            // Verify the index was created by the post-bootstrap migration step.
            SqliteConnection.ClearAllPools();
            using (var verify = new SqliteConnection($"Data Source={path}"))
            {
                verify.Open();
                using var cmd = verify.CreateCommand();
                cmd.CommandText = "SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = 'idx_qsos_deleted_at_ms'";
                using var reader = cmd.ExecuteReader();
                Assert.True(reader.Read(), "idx_qsos_deleted_at_ms should exist after migrating a legacy DB");
            }
        }
        finally
        {
            SqliteConnection.ClearAllPools();
            if (File.Exists(path))
            {
                try
                {
                    File.Delete(path);
                }
                catch (IOException)
                {
                }
            }
        }
    }
}
#pragma warning restore CA1707
