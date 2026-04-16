using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Engine.Storage;
using QsoRipper.Engine.Storage.Memory;

namespace QsoRipper.Engine.Storage.Memory.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class MemoryStorageTests
{
    private readonly MemoryStorage _storage = new();

    // ──────────────────────────────────────────────
    //  Backend metadata
    // ──────────────────────────────────────────────

    [Fact]
    public void BackendName_is_memory()
    {
        Assert.Equal("memory", _storage.BackendName);
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
    public async Task Insert_clones_input_so_mutations_do_not_affect_stored_data()
    {
        var qso = MakeQso("q1", "W1AW", Band._20M, Mode.Ft8, "2026-01-15T12:00:00Z");
        await _storage.Logbook.InsertQsoAsync(qso);

        qso.WorkedCallsign = "MUTATED";

        var loaded = await _storage.Logbook.GetQsoAsync("q1");
        Assert.Equal("W1AW", loaded!.WorkedCallsign);
    }

    [Fact]
    public async Task Get_returns_clone_so_mutations_do_not_affect_stored_data()
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

        Assert.Equal(2, result.Count);
        Assert.All(result, q => Assert.True(q.UtcTimestamp.ToDateTimeOffset() > DateTimeOffset.Parse("2026-01-15T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture)));
    }

    [Fact]
    public async Task List_filters_by_before()
    {
        await InsertThreeQsos();

        var result = await _storage.Logbook.ListQsosAsync(new QsoListQuery
        {
            Before = DateTimeOffset.Parse("2026-01-16T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture),
        });

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
        Assert.Equal(now, meta.LastSync);
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
    public async Task Lookup_clone_isolation()
    {
        await _storage.LookupSnapshots.UpsertAsync(MakeLookupSnapshot("W1AW"));

        var loaded = await _storage.LookupSnapshots.GetAsync("W1AW");
        loaded!.Result.QueriedCallsign = "MUTATED";

        var loadedAgain = await _storage.LookupSnapshots.GetAsync("W1AW");
        Assert.Equal("W1AW", loadedAgain!.Result.QueriedCallsign);
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
}
#pragma warning restore CA1707
