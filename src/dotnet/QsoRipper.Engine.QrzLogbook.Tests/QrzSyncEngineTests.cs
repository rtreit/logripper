using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Engine.QrzLogbook;
using QsoRipper.Engine.Storage;
using QsoRipper.Engine.Storage.Memory;

#pragma warning disable CA1307 // Use StringComparison for string comparison

namespace QsoRipper.Engine.QrzLogbook.Tests;

#pragma warning disable CA1707 // Remove underscores from member names

public sealed class QrzSyncEngineTests
{
    private static readonly DateTimeOffset BaseTime = new(2024, 6, 15, 12, 0, 0, TimeSpan.Zero);

    // -- Ghost filtering ----------------------------------------------------

    [Fact]
    public async Task Download_skips_ghost_records_missing_callsign()
    {
        var api = new FakeQrzLogbookApi
        {
            FetchResult =
            [
                MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "100"),
                MakeRemoteQso("", BaseTime, Band._20M, Mode.Ft8, "101"),       // ghost — empty callsign
                MakeRemoteQso("  ", BaseTime, Band._20M, Mode.Ft8, "102"),     // ghost — blank callsign
            ],
        };
        var store = CreateStore();
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.DownloadedCount);
    }

    [Fact]
    public async Task Download_skips_ghost_records_missing_timestamp()
    {
        var ghost = new QsoRecord { WorkedCallsign = "K5ABC", Band = Band._40M, Mode = Mode.Cw };
        // UtcTimestamp is null → ghost.
        var api = new FakeQrzLogbookApi
        {
            FetchResult = [ghost, MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "200")],
        };
        var store = CreateStore();
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.DownloadedCount);
    }

    // -- Phase 1: Download and insert new -----------------------------------

    [Fact]
    public async Task Download_inserts_new_remote_qsos()
    {
        var api = new FakeQrzLogbookApi
        {
            FetchResult =
            [
                MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "100"),
                MakeRemoteQso("K5ABC", BaseTime.AddMinutes(5), Band._40M, Mode.Cw, "101"),
            ],
        };
        var store = CreateStore();
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(2u, result.DownloadedCount);
        Assert.Equal(0u, result.UploadedCount);
        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Equal(2, allQsos.Count);
        Assert.All(allQsos, q => Assert.Equal(SyncStatus.Synced, q.SyncStatus));
    }

    [Fact]
    public async Task Download_new_qso_preserves_qrz_logid()
    {
        var api = new FakeQrzLogbookApi
        {
            FetchResult = [MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "999")],
        };
        var store = CreateStore();
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(allQsos);
        Assert.Equal("999", allQsos[0].QrzLogid);
    }

    // -- Phase 1: Download and merge ----------------------------------------

    [Fact]
    public async Task Download_merges_by_qrz_logid()
    {
        var store = CreateStore();
        // Seed a local QSO with a known logid.
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced);
        local.QrzLogid = "500";
        await store.Logbook.InsertQsoAsync(local);

        // Remote has same logid but different notes.
        var remote = MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "500");
        remote.Notes = "Updated from QRZ";

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.DownloadedCount);
        // Should have merged, not inserted a duplicate.
        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(allQsos);
        Assert.Equal("Updated from QRZ", allQsos[0].Notes);
        Assert.Equal(local.LocalId, allQsos[0].LocalId);
    }

    // -- Fuzzy matching -----------------------------------------------------

    [Fact]
    public async Task Download_fuzzy_matches_within_60_seconds()
    {
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(local);

        // Remote has same callsign/band/mode, timestamp 30s off, but no logid.
        var remote = MakeRemoteQso("W1AW", BaseTime.AddSeconds(30), Band._20M, Mode.Ft8, null);

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.DownloadedCount);
        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(allQsos);
        Assert.Equal(SyncStatus.Synced, allQsos[0].SyncStatus);
    }

    [Fact]
    public async Task Download_no_fuzzy_match_beyond_60_seconds()
    {
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(local);

        // Remote has same callsign/band/mode, but timestamp 90s off — too far.
        var remote = MakeRemoteQso("W1AW", BaseTime.AddSeconds(90), Band._20M, Mode.Ft8, null);

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.DownloadedCount);
        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Equal(2, allQsos.Count); // No match → inserted as new
    }

    [Fact]
    public async Task Download_fuzzy_match_requires_same_band_and_mode()
    {
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(local);

        // Same callsign and timestamp, but different band.
        var remote = MakeRemoteQso("W1AW", BaseTime, Band._40M, Mode.Ft8, null);

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Equal(2, allQsos.Count); // Different band → no match
    }

    // -- Phase 2: Upload pending --------------------------------------------

    [Fact]
    public async Task Upload_sends_local_only_qsos()
    {
        var store = CreateStore();
        var local = MakeLocalQso("N0CALL", BaseTime, Band._20M, Mode.Ssb, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(local);

        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            UploadLogid = "77777",
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.UploadedCount);
        Assert.Single(api.UploadedQsos);
        Assert.Equal("N0CALL", api.UploadedQsos[0].WorkedCallsign);

        // Verify local record is now synced with assigned logid.
        var allQsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(allQsos);
        Assert.Equal(SyncStatus.Synced, allQsos[0].SyncStatus);
        Assert.Equal("77777", allQsos[0].QrzLogid);
    }

    [Fact]
    public async Task Upload_sends_modified_qsos_via_replace()
    {
        var store = CreateStore();
        var local = MakeLocalQso("N0CALL", BaseTime, Band._20M, Mode.Ssb, SyncStatus.Modified);
        local.QrzLogid = "existing-id";
        await store.Logbook.InsertQsoAsync(local);

        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            UpdateLogid = "existing-id",
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.UploadedCount);
        // Modified QSO must go through UpdateQsoAsync (REPLACE), not UploadQsoAsync (INSERT)
        Assert.Empty(api.UploadedQsos);
        Assert.Single(api.UpdatedQsos);
        Assert.Equal("N0CALL", api.UpdatedQsos[0].WorkedCallsign);
    }

    [Fact]
    public async Task Upload_modified_qso_overwrites_existing_logid_bug_213()
    {
        // Regression: modified QSOs with an existing QrzLogid must use REPLACE, not INSERT.
        var store = CreateStore();
        var modified = MakeLocalQso("W1AW", BaseTime, Band._40M, Mode.Cw, SyncStatus.Modified);
        modified.QrzLogid = "qrz-99";
        await store.Logbook.InsertQsoAsync(modified);

        var newQso = MakeLocalQso("K1ABC", BaseTime.AddMinutes(5), Band._20M, Mode.Ssb, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(newQso);

        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            UploadLogid = "new-id-100",
            UpdateLogid = "qrz-99",
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(2u, result.UploadedCount);

        // Modified QSO → UpdateQsoAsync (REPLACE)
        Assert.Single(api.UpdatedQsos);
        Assert.Equal("W1AW", api.UpdatedQsos[0].WorkedCallsign);

        // New QSO → UploadQsoAsync (INSERT)
        Assert.Single(api.UploadedQsos);
        Assert.Equal("K1ABC", api.UploadedQsos[0].WorkedCallsign);
    }

    [Fact]
    public async Task Upload_does_not_send_already_synced_qsos()
    {
        var store = CreateStore();
        var local = MakeLocalQso("N0CALL", BaseTime, Band._20M, Mode.Ssb, SyncStatus.Synced);
        local.QrzLogid = "already-synced";
        await store.Logbook.InsertQsoAsync(local);

        var api = new FakeQrzLogbookApi { FetchResult = [] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(0u, result.UploadedCount);
        Assert.Empty(api.UploadedQsos);
    }

    [Fact]
    public async Task Upload_partial_failure_continues_other_qsos()
    {
        var store = CreateStore();
        await store.Logbook.InsertQsoAsync(MakeLocalQso("FIRST", BaseTime, Band._20M, Mode.Ssb, SyncStatus.LocalOnly));
        await store.Logbook.InsertQsoAsync(MakeLocalQso("SECOND", BaseTime.AddMinutes(1), Band._40M, Mode.Cw, SyncStatus.LocalOnly));

        var callCount = 0;
        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            UploadLogid = "99",
            UploadFunc = qso =>
            {
                callCount++;
                return callCount == 1
                    ? throw new QrzLogbookException("transient error")
                    : Task.FromResult("99");
            },
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.UploadedCount);
        Assert.NotNull(result.ErrorSummary);
        Assert.Contains("Upload failed for FIRST", result.ErrorSummary);
    }

    // -- Phase 3: Metadata --------------------------------------------------

    [Fact]
    public async Task Sync_updates_metadata_last_sync()
    {
        var store = CreateStore();
        var api = new FakeQrzLogbookApi { FetchResult = [] };
        var engine = new QrzSyncEngine(api);

        var before = await store.Logbook.GetSyncMetadataAsync();
        Assert.Null(before.LastSync);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        var after = await store.Logbook.GetSyncMetadataAsync();
        Assert.NotNull(after.LastSync);
        Assert.True(after.LastSync.Value > DateTimeOffset.UtcNow.AddMinutes(-1));
    }

    [Fact]
    public async Task Sync_populates_metadata_from_status_call()
    {
        var store = CreateStore();
        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            StatusOwner = "KC7AVA",
            StatusQsoCount = 1234,
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1, api.StatusCallCount);
        Assert.Equal(1234u, result.RemoteQsoCount);
        Assert.Equal("KC7AVA", result.RemoteOwner);

        var meta = await store.Logbook.GetSyncMetadataAsync();
        Assert.Equal(1234, meta.QrzQsoCount);
        Assert.Equal("KC7AVA", meta.QrzLogbookOwner);
    }

    [Fact]
    public async Task Sync_falls_back_to_local_count_when_status_fails()
    {
        var store = CreateStore();

        // Seed one already-synced local QSO; after sync, total local == 1, pending == 0,
        // so local fallback count = 1.
        var synced = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced);
        synced.QrzLogid = "1001";
        await store.Logbook.InsertQsoAsync(synced);

        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            StatusException = new InvalidOperationException("STATUS unavailable"),
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Null(result.RemoteQsoCount);
        Assert.NotNull(result.ErrorSummary);
        Assert.Contains("STATUS refresh failed", result.ErrorSummary);

        var meta = await store.Logbook.GetSyncMetadataAsync();
        Assert.Equal(1, meta.QrzQsoCount);
    }

    [Fact]
    public async Task Sync_preserves_previous_owner_when_status_returns_empty_owner()
    {
        var store = CreateStore();
        await store.Logbook.UpsertSyncMetadataAsync(new SyncMetadata
        {
            QrzLogbookOwner = "W1AW",
            QrzQsoCount = 10,
        });

        var api = new FakeQrzLogbookApi
        {
            FetchResult = [],
            StatusOwner = string.Empty,
            StatusQsoCount = 11,
        };
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        var meta = await store.Logbook.GetSyncMetadataAsync();
        Assert.Equal("W1AW", meta.QrzLogbookOwner);
        Assert.Equal(11, meta.QrzQsoCount);
    }

    // -- Incremental vs full ------------------------------------------------

    [Fact]
    public async Task Incremental_sync_passes_since_date()
    {
        var store = CreateStore();
        // Seed metadata with a past sync date.
        await store.Logbook.UpsertSyncMetadataAsync(new SyncMetadata
        {
            LastSync = new DateTimeOffset(2024, 6, 1, 0, 0, 0, TimeSpan.Zero),
        });
        // Need at least one local QSO so the engine doesn't force full sync.
        await store.Logbook.InsertQsoAsync(MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced));

        var api = new FakeQrzLogbookApi { FetchResult = [] };
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: false);

        Assert.Equal("2024-06-01", api.LastSinceDate);
    }

    [Fact]
    public async Task Full_sync_passes_null_since_date()
    {
        var store = CreateStore();
        await store.Logbook.UpsertSyncMetadataAsync(new SyncMetadata
        {
            LastSync = new DateTimeOffset(2024, 6, 1, 0, 0, 0, TimeSpan.Zero),
        });
        await store.Logbook.InsertQsoAsync(MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced));

        var api = new FakeQrzLogbookApi { FetchResult = [] };
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Null(api.LastSinceDate);
    }

    [Fact]
    public async Task Empty_local_logbook_forces_full_fetch_even_when_incremental()
    {
        var store = CreateStore();
        await store.Logbook.UpsertSyncMetadataAsync(new SyncMetadata
        {
            LastSync = new DateTimeOffset(2024, 6, 1, 0, 0, 0, TimeSpan.Zero),
        });
        // No local QSOs → should force null since date.

        var api = new FakeQrzLogbookApi { FetchResult = [] };
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: false);

        Assert.Null(api.LastSinceDate);
    }

    // -- Logid extraction helper --------------------------------------------

    [Fact]
    public void ExtractQrzLogid_prefers_dedicated_field()
    {
        var qso = new QsoRecord { QrzLogid = "direct-id" };
        qso.ExtraFields["APP_QRZ_LOGID"] = "extra-id";

        Assert.Equal("direct-id", QrzSyncEngine.ExtractQrzLogid(qso));
    }

    [Fact]
    public void ExtractQrzLogid_falls_back_to_extra_field()
    {
        var qso = new QsoRecord();
        qso.ExtraFields["APP_QRZLOG_LOGID"] = "alt-id";

        Assert.Equal("alt-id", QrzSyncEngine.ExtractQrzLogid(qso));
    }

    [Fact]
    public void ExtractQrzLogid_returns_null_when_absent()
    {
        var qso = new QsoRecord();

        Assert.Null(QrzSyncEngine.ExtractQrzLogid(qso));
    }

    // -- ConflictPolicy -----------------------------------------------------

    [Fact]
    public async Task Merge_last_write_wins_overwrites_local_modifications()
    {
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Modified);
        local.QrzLogid = "700";
        local.Notes = "Local edit that should be overwritten";
        await store.Logbook.InsertQsoAsync(local);

        var remote = MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "700");
        remote.Notes = "Remote copy";

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(
            store.Logbook,
            fullSync: true,
            ConflictPolicy.LastWriteWins);

        Assert.Equal(0u, result.ConflictCount);
        var all = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(all);
        Assert.Equal("Remote copy", all[0].Notes);
        Assert.Equal(SyncStatus.Synced, all[0].SyncStatus);
    }

    [Fact]
    public async Task Merge_last_write_wins_adopts_remote_qrz_logid_when_differs()
    {
        // Regression for #161: when remote wins under LastWriteWins, the
        // overwritten local row must carry the REMOTE qrz_logid (not the
        // stale local one), otherwise the next sync would point at a phantom.
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Modified);
        local.QrzLogid = "LOG-LOCAL-OLD";
        local.Notes = "local stale";
        await store.Logbook.InsertQsoAsync(local);

        var remote = MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "LOG-REMOTE-NEW");
        remote.Notes = "remote authoritative";

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(
            store.Logbook,
            fullSync: true,
            ConflictPolicy.LastWriteWins);

        Assert.Equal(0u, result.ConflictCount);
        var all = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(all);
        Assert.Equal("LOG-REMOTE-NEW", all[0].QrzLogid);
    }

    [Fact]
    public async Task Merge_flag_for_review_preserves_local_and_marks_conflict()
    {
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Modified);
        local.QrzLogid = "800";
        local.Notes = "Local operator edit";
        await store.Logbook.InsertQsoAsync(local);

        var remote = MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "800");
        remote.Notes = "Remote that should NOT win";

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(
            store.Logbook,
            fullSync: true,
            ConflictPolicy.FlagForReview);

        Assert.Equal(1u, result.ConflictCount);
        var all = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(all);
        Assert.Equal("Local operator edit", all[0].Notes);
        Assert.Equal(SyncStatus.Conflict, all[0].SyncStatus);
        Assert.Equal("800", all[0].QrzLogid);
    }

    [Fact]
    public async Task Merge_unspecified_policy_defaults_to_flag_for_review()
    {
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Modified);
        local.QrzLogid = "900";
        local.Notes = "Local edit";
        await store.Logbook.InsertQsoAsync(local);

        var remote = MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "900");
        remote.Notes = "Remote copy";

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        // Unspecified must act like FlagForReview per engine spec §6.3.
        var result = await engine.ExecuteSyncAsync(
            store.Logbook,
            fullSync: true,
            ConflictPolicy.Unspecified);

        Assert.Equal(1u, result.ConflictCount);
        var all = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Equal("Local edit", all[0].Notes);
        Assert.Equal(SyncStatus.Conflict, all[0].SyncStatus);
    }

    [Fact]
    public async Task Merge_synced_local_always_accepts_remote_regardless_of_policy()
    {
        // When local is already Synced (no user edits since last sync), the
        // remote is authoritative no matter what the conflict policy is, and
        // the row is not counted as a conflict.
        var store = CreateStore();
        var local = MakeLocalQso("W1AW", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced);
        local.QrzLogid = "1000";
        local.Notes = "Old synced value";
        await store.Logbook.InsertQsoAsync(local);

        var remote = MakeRemoteQso("W1AW", BaseTime, Band._20M, Mode.Ft8, "1000");
        remote.Notes = "Fresh from QRZ";

        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(
            store.Logbook,
            fullSync: true,
            ConflictPolicy.FlagForReview);

        Assert.Equal(0u, result.ConflictCount);
        var all = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Equal("Fresh from QRZ", all[0].Notes);
        Assert.Equal(SyncStatus.Synced, all[0].SyncStatus);
    }

    // -- Soft-delete sync integration ---------------------------------------

    [Fact]
    public async Task Download_skips_remote_matching_soft_deleted_local()
    {
        // A previously-synced local row was soft-deleted. The next sync
        // must NOT resurrect it from QRZ.
        var store = CreateStore();
        var local = MakeLocalQso("K7ABC", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced);
        local.QrzLogid = "LOG-DELETED";
        await store.Logbook.InsertQsoAsync(local);
        await store.Logbook.SoftDeleteQsoAsync(local.LocalId, DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var remote = MakeRemoteQso("K7ABC", BaseTime, Band._20M, Mode.Ft8, "LOG-DELETED");
        var api = new FakeQrzLogbookApi { FetchResult = [remote] };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(0u, result.DownloadedCount);
        Assert.Equal(1u, result.DeletesSkippedRemote);

        var all = await store.Logbook.ListQsosAsync(new QsoListQuery
        {
            DeletedFilter = DeletedRecordsFilter.All,
        });
        Assert.Single(all);
        Assert.NotNull(all[0].DeletedAt);
    }

    [Fact]
    public async Task PushPendingRemoteDeletes_calls_qrz_and_clears_local_flags()
    {
        var store = CreateStore();
        var local = MakeLocalQso("JA1ZZZ", BaseTime, Band._40M, Mode.Cw, SyncStatus.Synced);
        local.QrzLogid = "LOG-PENDING";
        await store.Logbook.InsertQsoAsync(local);
        await store.Logbook.SoftDeleteQsoAsync(local.LocalId, DateTimeOffset.UtcNow, pendingRemoteDelete: true);

        var api = new FakeQrzLogbookApi();
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.RemoteDeletesPushed);
        Assert.Null(result.ErrorSummary);
        Assert.Single(api.DeletedLogids);
        Assert.Equal("LOG-PENDING", api.DeletedLogids[0]);

        var all = await store.Logbook.ListQsosAsync(new QsoListQuery
        {
            DeletedFilter = DeletedRecordsFilter.All,
        });
        Assert.Single(all);
        Assert.NotNull(all[0].DeletedAt);
        Assert.False(all[0].PendingRemoteDelete);
        Assert.True(string.IsNullOrEmpty(all[0].QrzLogid));
    }

    [Fact]
    public async Task PushPendingRemoteDeletes_does_not_call_when_pending_flag_unset()
    {
        var store = CreateStore();
        var local = MakeLocalQso("K7ABC", BaseTime, Band._20M, Mode.Ft8, SyncStatus.Synced);
        local.QrzLogid = "LOG-LOCAL-ONLY-TRASH";
        await store.Logbook.InsertQsoAsync(local);
        await store.Logbook.SoftDeleteQsoAsync(local.LocalId, DateTimeOffset.UtcNow, pendingRemoteDelete: false);

        var api = new FakeQrzLogbookApi();
        var engine = new QrzSyncEngine(api);

        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Empty(api.DeletedLogids);
    }

    [Fact]
    public async Task PushPendingRemoteDeletes_preserves_state_on_failure()
    {
        var store = CreateStore();
        var local = MakeLocalQso("DL1ABC", BaseTime, Band._20M, Mode.Ssb, SyncStatus.Synced);
        local.QrzLogid = "LOG-FAIL";
        await store.Logbook.InsertQsoAsync(local);
        await store.Logbook.SoftDeleteQsoAsync(local.LocalId, DateTimeOffset.UtcNow, pendingRemoteDelete: true);

        var api = new FakeQrzLogbookApi
        {
            DeleteException = new QrzLogbookException("server angry"),
        };
        var engine = new QrzSyncEngine(api);

        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(0u, result.RemoteDeletesPushed);
        Assert.NotNull(result.ErrorSummary);

        var all = await store.Logbook.ListQsosAsync(new QsoListQuery
        {
            DeletedFilter = DeletedRecordsFilter.All,
        });
        Assert.Single(all);
        Assert.NotNull(all[0].DeletedAt);
        Assert.True(all[0].PendingRemoteDelete);
        Assert.Equal("LOG-FAIL", all[0].QrzLogid);
    }

    // -- Issue #337: previous-callsign rewrite ------------------------------

    [Fact]
    public async Task Upload_rewrites_station_callsign_to_book_owner_for_previous_call()
    {
        var api = new FakeQrzLogbookApi { StatusOwner = "AE7XI" };
        var store = CreateStore();
        var local = MakeLocalQso("K7ABC", BaseTime, Band._20M, Mode.Cw, SyncStatus.LocalOnly);
        local.StationCallsign = "KB7QOP"; // historical previous call
        await store.Logbook.InsertQsoAsync(local);

        var engine = new QrzSyncEngine(api);
        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1u, result.UploadedCount);
        Assert.Equal("AE7XI", api.LastUploadBookOwner);

        var uploaded = Assert.Single(api.UploadedQsos);
        // QrzLogbookClient (real) clones; the engine forwards the live qso.
        // What matters is that the *bookOwner argument* is correct AND that
        // RewriteStationCallsignForBook is what the client applies. Verify
        // the helper directly:
        AdifCodec.RewriteStationCallsignForBook(uploaded, api.LastUploadBookOwner);
        Assert.Equal("AE7XI", uploaded.StationCallsign);
        Assert.NotNull(uploaded.StationSnapshot);
        Assert.Equal("AE7XI", uploaded.StationSnapshot!.StationCallsign);
        Assert.Equal("KB7QOP", uploaded.StationSnapshot.OperatorCallsign);

        // Local row must remain with the historical station callsign.
        var localList = await store.Logbook.ListQsosAsync(new QsoListQuery());
        var saved = Assert.Single(localList);
        Assert.Equal("KB7QOP", saved.StationCallsign);
    }

    [Fact]
    public async Task Upload_falls_back_to_cached_owner_when_status_fails()
    {
        var api = new FakeQrzLogbookApi
        {
            StatusException = new InvalidOperationException("transient"),
        };
        var store = CreateStore();
        await store.Logbook.UpsertSyncMetadataAsync(new SyncMetadata { QrzLogbookOwner = "AE7XI" });
        var local = MakeLocalQso("K7ABC", BaseTime, Band._20M, Mode.Cw, SyncStatus.LocalOnly);
        local.StationCallsign = "KB7QOP";
        await store.Logbook.InsertQsoAsync(local);

        var engine = new QrzSyncEngine(api);
        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal("AE7XI", api.LastUploadBookOwner);
    }

    [Fact]
    public async Task Upload_makes_only_one_status_call_per_sync()
    {
        // Regression: STATUS used to be called twice (once for owner-resolution
        // before upload, once for metadata refresh in Phase 3). Phase 3 must
        // reuse the Phase 1.5 result so we don't double-bill the QRZ API.
        var api = new FakeQrzLogbookApi { StatusOwner = "K7TEST" };
        var store = CreateStore();
        await store.Logbook.InsertQsoAsync(
            MakeLocalQso("K7DEST", BaseTime, Band._20M, Mode.Ft8, SyncStatus.LocalOnly));

        var engine = new QrzSyncEngine(api);
        await engine.ExecuteSyncAsync(store.Logbook, fullSync: true);

        Assert.Equal(1, api.StatusCallCount);
    }

    [Fact]
    public void RewriteStationCallsignForBook_skips_slash_suffix_calls()
    {
        var qso = MakeLocalQso("K7ABC", BaseTime, Band._20M, Mode.Cw, SyncStatus.LocalOnly);
        qso.StationCallsign = "KB7QOP/P";

        AdifCodec.RewriteStationCallsignForBook(qso, "AE7XI");

        Assert.Equal("KB7QOP/P", qso.StationCallsign);
    }

    [Fact]
    public void RewriteStationCallsignForBook_does_not_overwrite_existing_operator()
    {
        var qso = MakeLocalQso("K7ABC", BaseTime, Band._20M, Mode.Cw, SyncStatus.LocalOnly);
        qso.StationCallsign = "KB7QOP";
        qso.StationSnapshot = new StationSnapshot
        {
            StationCallsign = "KB7QOP",
            OperatorCallsign = "W1ZZZ",
        };

        AdifCodec.RewriteStationCallsignForBook(qso, "AE7XI");

        Assert.Equal("AE7XI", qso.StationCallsign);
        Assert.Equal("W1ZZZ", qso.StationSnapshot.OperatorCallsign);
    }

    // -- Helpers ------------------------------------------------------------

    // -- Duplicate retry tests -----------------------------------------------

    [Fact]
    public async Task Upload_duplicate_retries_with_replace_and_adopts_logid()
    {
        // A local QSO exists on QRZ but has no qrz_logid locally. The plain
        // INSERT fails with "duplicate". Sync should retry with REPLACE, adopt
        // the returned LOGID, and mark the QSO as Synced.
        var store = CreateStore();
        var local = MakeLocalQso("AK7S", BaseTime, Band._20M, Mode.Ft8, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(local);

        var api = new FakeQrzLogbookApi
        {
            UploadFunc = _ => throw new QrzLogbookException("QRZ Logbook API error: Unable to add QSO to database: duplicate"),
            UploadReplaceLogid = "QRZ_ADOPTED_456",
        };

        var engine = new QrzSyncEngine(api);
        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: false);

        // No errors — duplicate was handled.
        Assert.Null(result.ErrorSummary);
        Assert.Equal(1u, result.UploadedCount);

        // REPLACE retry was called.
        Assert.Single(api.UploadReplacedQsos);
        Assert.Equal("AK7S", api.UploadReplacedQsos[0].WorkedCallsign);

        // Local QSO is now Synced with the adopted LOGID.
        var qsos = await store.Logbook.ListQsosAsync(new QsoListQuery());
        Assert.Single(qsos);
        Assert.Equal(SyncStatus.Synced, qsos[0].SyncStatus);
        Assert.Equal("QRZ_ADOPTED_456", qsos[0].QrzLogid);
    }

    [Fact]
    public async Task Upload_non_duplicate_error_still_reported()
    {
        var store = CreateStore();
        var local = MakeLocalQso("K3SEW", BaseTime, Band._40M, Mode.Ssb, SyncStatus.LocalOnly);
        await store.Logbook.InsertQsoAsync(local);

        var api = new FakeQrzLogbookApi
        {
            UploadFunc = _ => throw new QrzLogbookException("invalid ADIF record"),
        };

        var engine = new QrzSyncEngine(api);
        var result = await engine.ExecuteSyncAsync(store.Logbook, fullSync: false);

        Assert.NotNull(result.ErrorSummary);
        Assert.Contains("invalid ADIF record", result.ErrorSummary);
        Assert.Equal(0u, result.UploadedCount);

        // REPLACE retry must NOT have been called.
        Assert.Empty(api.UploadReplacedQsos);
    }

    private static MemoryStorage CreateStore() => new();

    private static QsoRecord MakeRemoteQso(string callsign, DateTimeOffset timestamp, Band band, Mode mode, string? logid)
    {
        var qso = new QsoRecord
        {
            LocalId = Guid.NewGuid().ToString(),
            WorkedCallsign = callsign,
            Band = band,
            Mode = mode,
            UtcTimestamp = Timestamp.FromDateTimeOffset(timestamp),
        };

        if (logid is not null)
        {
            qso.QrzLogid = logid;
        }

        return qso;
    }

    private static QsoRecord MakeLocalQso(string callsign, DateTimeOffset timestamp, Band band, Mode mode, SyncStatus status)
    {
        return new QsoRecord
        {
            LocalId = Guid.NewGuid().ToString(),
            WorkedCallsign = callsign,
            Band = band,
            Mode = mode,
            UtcTimestamp = Timestamp.FromDateTimeOffset(timestamp),
            SyncStatus = status,
        };
    }

    /// <summary>
    /// Test double for <see cref="IQrzLogbookApi"/>.
    /// </summary>
    internal sealed class FakeQrzLogbookApi : IQrzLogbookApi
    {
        public List<QsoRecord> FetchResult { get; set; } = [];
        public string UploadLogid { get; set; } = "12345";
        public string UpdateLogid { get; set; } = "12345";
        public Func<QsoRecord, Task<string>>? UploadFunc { get; set; }
        public List<QsoRecord> UploadedQsos { get; } = [];
        public List<QsoRecord> UpdatedQsos { get; } = [];
        public string? LastSinceDate { get; private set; }

        public Task<List<QsoRecord>> FetchQsosAsync(string? sinceDateYmd)
        {
            LastSinceDate = sinceDateYmd;
            return Task.FromResult(FetchResult);
        }

        public string? LastUploadBookOwner { get; private set; }

        public string? LastUpdateBookOwner { get; private set; }

        public Task<string> UploadQsoAsync(QsoRecord qso, string? bookOwner = null)
        {
            UploadedQsos.Add(qso);
            LastUploadBookOwner = bookOwner;
            if (UploadFunc is not null)
            {
                return UploadFunc(qso);
            }

            return Task.FromResult(UploadLogid);
        }

        public string UploadReplaceLogid { get; set; } = "REPLACE-12345";
        public List<QsoRecord> UploadReplacedQsos { get; } = [];

        public Task<string> UploadQsoWithReplaceAsync(QsoRecord qso, string? bookOwner = null)
        {
            UploadReplacedQsos.Add(qso);
            return Task.FromResult(UploadReplaceLogid);
        }

        public Task<string> UpdateQsoAsync(QsoRecord qso, string? bookOwner = null)
        {
            UpdatedQsos.Add(qso);
            LastUpdateBookOwner = bookOwner;
            return Task.FromResult(UpdateLogid);
        }

        /// <summary>Configurable STATUS owner. Empty string mimics QRZ omitting the field.</summary>
        public string StatusOwner { get; set; } = "K7TEST";

        /// <summary>Configurable STATUS QSO count.</summary>
        public uint StatusQsoCount { get; set; }

        /// <summary>When non-null, <see cref="GetStatusAsync"/> throws this instead of returning.</summary>
        public Exception? StatusException { get; set; }

        public int StatusCallCount { get; private set; }

        public Task<QrzLogbookStatus> GetStatusAsync()
        {
            StatusCallCount++;
            if (StatusException is not null)
            {
                return Task.FromException<QrzLogbookStatus>(StatusException);
            }

            return Task.FromResult(new QrzLogbookStatus(StatusOwner, StatusQsoCount));
        }

        public List<string> DeletedLogids { get; } = [];

        public Exception? DeleteException { get; set; }

        public Task DeleteQsoAsync(string logid)
        {
            DeletedLogids.Add(logid);
            if (DeleteException is not null)
            {
                return Task.FromException(DeleteException);
            }

            return Task.CompletedTask;
        }
    }
}
