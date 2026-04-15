using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

public class RecentQsoListViewModelTests
{
    [Fact]
    public async Task RefreshAsyncLoadsDenseColumnsAndSelectsFirstItem()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso(
                    "qso-1",
                    "W1AW",
                    Band._40M,
                    Mode.Cw,
                    7025,
                    "CN87",
                    "First recent QSO",
                    operatorName: "Alice",
                    state: "WA",
                    country: "United States",
                    dxcc: 291,
                    contestId: "CQ-WW",
                    exchangeReceived: "WA",
                    utcTimestamp: new DateTimeOffset(2026, 4, 13, 22, 16, 0, TimeSpan.Zero)),
                CreateQso(
                    "qso-2",
                    "K7RND",
                    Band._20M,
                    Mode.Ft8,
                    14074,
                    "CN88",
                    "Second recent QSO",
                    operatorName: "Bob",
                    state: "BC",
                    country: "Canada",
                    dxcc: 1,
                    contestId: "POTA",
                    exchangeReceived: "BC",
                    utcTimestamp: new DateTimeOffset(2026, 4, 13, 22, 15, 0, TimeSpan.Zero),
                    syncStatus: SyncStatus.Synced)
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);

        await viewModel.RefreshAsync();

        Assert.True(viewModel.HasVisibleItems);
        Assert.Equal(2, viewModel.VisibleItems.Count);
        Assert.Equal("qso-1", viewModel.SelectedQso?.LocalId);
        Assert.Equal("40M", viewModel.VisibleItems[0].Band);
        Assert.Equal("CW", viewModel.VisibleItems[0].Mode);
        Assert.Equal("Alice", viewModel.VisibleItems[0].OperatorName);
        Assert.Equal("United States", viewModel.VisibleItems[0].Country);
        Assert.Equal("59/57", viewModel.VisibleItems[0].Rst);
        Assert.Equal("291", viewModel.VisibleItems[0].Dxcc);
        Assert.Equal("WA", viewModel.VisibleItems[0].Exchange);
        Assert.Equal("CQ-WW", viewModel.VisibleItems[0].Contest);
        Assert.Equal("K7RND", viewModel.VisibleItems[0].Station);
        Assert.Equal("Sort: UTC time desc", viewModel.SortStatusText);
        Assert.Equal("2 QSOs", viewModel.CountStatusText);
        Assert.Equal("No filter", viewModel.FilterStatusText);
        Assert.Equal("Sync local", viewModel.TopSyncIndicatorText);
    }

    [Fact]
    public async Task SearchTextFiltersAcrossMultipleColumnsAndTracksTokens()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso(
                    "qso-1",
                    "W1AW",
                    Band._20M,
                    Mode.Ft8,
                    14074,
                    "CN87",
                    "CQ test",
                    operatorName: "Alice",
                    state: "WA",
                    country: "United States",
                    contestId: "POTA",
                    exchangeReceived: "WA"),
                CreateQso(
                    "qso-2",
                    "VE7ABC",
                    Band._40M,
                    Mode.Ssb,
                    7185,
                    "CN88",
                    "Morning ragchew",
                    operatorName: "Bob",
                    state: "BC",
                    country: "Canada",
                    contestId: "CQ-WW",
                    exchangeReceived: "BC")
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        viewModel.SearchText = "call:w1aw contest:pota wa";

        Assert.Single(viewModel.VisibleItems);
        Assert.Equal("qso-1", viewModel.VisibleItems[0].LocalId);
        Assert.Equal(3, viewModel.ActiveFilterTokens.Count);
        Assert.Equal("1 filtered", viewModel.FilterStatusText);
    }

    [Fact]
    public async Task SaveEditsAsyncUpdatesDirtyRowsAndClearsPendingEdits()
    {
        var original = CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "Loaded", operatorName: "Alice", state: "WA", country: "United States");
        var updated = original.Clone();
        updated.Comment = "Updated note";

        var engine = new FakeEngineClient
        {
            RecentQsos = [original]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        viewModel.SelectedQso!.BeginEdit();
        viewModel.SelectedQso.Note = "Updated note";
        viewModel.SelectedQso.EndEdit();

        Assert.Equal(1, viewModel.PendingEditCount);
        Assert.False(viewModel.RefreshCommand.CanExecute(null));

        engine.RecentQsos = [updated];
        await viewModel.SaveEditsCommand.ExecuteAsync(null);

        Assert.Single(engine.UpdatedQsos);
        Assert.Equal("Updated note", engine.UpdatedQsos[0].Comment);
        Assert.Equal(0, viewModel.PendingEditCount);
        Assert.Equal("No pending edits", viewModel.EditStatusText);
        Assert.Equal("Updated note", viewModel.SelectedQso?.Note);
    }

    [Fact]
    public async Task SaveEditsAsyncRejectsInvalidBandAndKeepsDirtyRow()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "Loaded", operatorName: "Alice", state: "WA", country: "United States")
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        viewModel.SelectedQso!.BeginEdit();
        viewModel.SelectedQso.Band = "bogus";
        viewModel.SelectedQso.EndEdit();

        await viewModel.SaveEditsCommand.ExecuteAsync(null);

        Assert.Equal("Invalid band: bogus.", viewModel.ErrorMessage);
        Assert.Empty(engine.UpdatedQsos);
        Assert.Equal(1, viewModel.PendingEditCount);
        Assert.True(viewModel.SelectedQso?.IsDirty);
    }

    [Fact]
    public async Task RefreshAsyncKeepsPreviousRowsWhenEngineRefreshFails()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "Loaded", operatorName: "Alice", state: "WA", country: "United States")
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        engine.RefreshException = new RpcException(new Status(StatusCode.Unavailable, "Engine unavailable"));
        await viewModel.RefreshAsync();

        Assert.Single(viewModel.VisibleItems);
        Assert.Equal("qso-1", viewModel.VisibleItems[0].LocalId);
        Assert.Equal("Engine unavailable", viewModel.ErrorMessage);
    }

    [Fact]
    public void MatchesSearchSupportsFieldTokensAgainstDenseColumns()
    {
        var item = RecentQsoItemViewModel.FromQso(
            CreateQso(
                "qso-1",
                "W1AW",
                Band._40M,
                Mode.Cw,
                7025,
                "CN87",
                "Evening CW",
                operatorName: "Alice",
                state: "WA",
                country: "United States",
                contestId: "CQ-WW",
                exchangeReceived: "WA"));

        Assert.True(RecentQsoListViewModel.MatchesSearch(item, "call:w1aw band:40m contest:cq note:evening"));
        Assert.False(RecentQsoListViewModel.MatchesSearch(item, "call:w1aw band:20m"));
    }

    [Fact]
    public async Task ApplySortOrdersByCallsignAndTogglesDirection()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-2", "VE7ABC", Band._40M, Mode.Ssb, 7185, "CN88", "Second", operatorName: "Bob", state: "BC", country: "Canada"),
                CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "First", operatorName: "Alice", state: "WA", country: "United States")
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        viewModel.ApplySort(RecentQsoSortColumn.Callsign);
        Assert.Equal("VE7ABC", viewModel.VisibleItems[0].WorkedCallsign);
        Assert.Equal("Sort: callsign asc", viewModel.SortStatusText);

        viewModel.ApplySort(RecentQsoSortColumn.Callsign);
        Assert.Equal("W1AW", viewModel.VisibleItems[0].WorkedCallsign);
        Assert.Equal("Sort: callsign desc", viewModel.SortStatusText);
    }

    [Fact]
    public async Task ReverseCurrentSortDirectionReversesTheCurrentSort()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "First", operatorName: "Alice", state: "WA", country: "United States"),
                CreateQso("qso-2", "K7RND", Band._40M, Mode.Ssb, 7185, "CN88", "Second", operatorName: "Bob", state: "BC", country: "Canada")
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();
        viewModel.ApplySort(RecentQsoSortColumn.Country);

        var firstAscending = viewModel.VisibleItems[0].Country;
        viewModel.ReverseCurrentSortDirection();

        Assert.NotEqual(firstAscending, viewModel.VisibleItems[0].Country);
    }

    [Fact]
    public async Task SyncSummaryReflectsMixedStatuses()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "First", syncStatus: SyncStatus.LocalOnly),
                CreateQso("qso-2", "K7RND", Band._40M, Mode.Ssb, 7185, "CN88", "Second", syncStatus: SyncStatus.Modified),
                CreateQso("qso-3", "VE7ABC", Band._15M, Mode.Ft8, 21074, "CN89", "Third", syncStatus: SyncStatus.Conflict),
                CreateQso("qso-4", "N0CALL", Band._10M, Mode.Ssb, 28450, "EN34", "Fourth", syncStatus: SyncStatus.Synced),
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        Assert.Equal("Sync: 1 local | 1 modified | 1 conflict | 1 synced", viewModel.SyncSummaryText);
        Assert.Equal("Sync conflict", viewModel.TopSyncIndicatorText);
    }

    [Fact]
    public void CancelEditRestoresSnapshotAndClearsDirtyState()
    {
        var item = RecentQsoItemViewModel.FromQso(
            CreateQso("qso-1", "W1AW", Band._20M, Mode.Cw, 14025, "CN87", "Loaded", operatorName: "Alice", state: "WA", country: "United States"));

        item.BeginEdit();
        item.WorkedCallsign = "VE7ABC";
        item.CancelEdit();

        Assert.Equal("W1AW", item.WorkedCallsign);
        Assert.False(item.IsDirty);
    }

    [Fact]
    public void AdjustZoomClampsFontSizeAndUpdatesStatus()
    {
        var viewModel = new RecentQsoListViewModel(new FakeEngineClient());

        Assert.Equal(12, viewModel.GridFontSize);
        Assert.Equal("Zoom 100%", viewModel.GridZoomStatusText);

        Assert.True(viewModel.AdjustZoom(1));
        Assert.Equal(13, viewModel.GridFontSize);
        Assert.Equal("Zoom 108%", viewModel.GridZoomStatusText);

        viewModel.ApplyPersistedGridFontSize(99);
        Assert.Equal(18, viewModel.GridFontSize);

        Assert.False(viewModel.AdjustZoom(1));

        viewModel.ResetGridZoom();
        Assert.Equal(12, viewModel.GridFontSize);
    }

    private static QsoRecord CreateQso(
        string localId,
        string workedCallsign,
        Band band,
        Mode mode,
        ulong frequencyKhz,
        string grid,
        string comment,
        string? operatorName = null,
        string? state = null,
        string? country = null,
        uint dxcc = 0,
        string? contestId = null,
        string? exchangeReceived = null,
        string? notes = null,
        DateTimeOffset? utcTimestamp = null,
        SyncStatus syncStatus = SyncStatus.LocalOnly)
    {
        return new QsoRecord
        {
            LocalId = localId,
            WorkedCallsign = workedCallsign,
            StationCallsign = "K7RND",
            UtcTimestamp = Timestamp.FromDateTimeOffset(utcTimestamp ?? new DateTimeOffset(2026, 4, 13, 22, 15, 0, TimeSpan.Zero)),
            Band = band,
            Mode = mode,
            FrequencyKhz = frequencyKhz,
            WorkedGrid = grid,
            Comment = comment,
            Notes = notes ?? string.Empty,
            ContestId = contestId ?? string.Empty,
            ExchangeReceived = exchangeReceived ?? string.Empty,
            WorkedOperatorName = operatorName ?? string.Empty,
            WorkedState = state ?? string.Empty,
            WorkedCountry = country ?? string.Empty,
            WorkedDxcc = dxcc,
            RstSent = new RstReport { Raw = "59" },
            RstReceived = new RstReport { Raw = "57" },
            SyncStatus = syncStatus
        };
    }

    private sealed class FakeEngineClient : IEngineClient
    {
        public IReadOnlyList<QsoRecord> RecentQsos { get; set; } = [];

        public RpcException? RefreshException { get; set; }

        public List<QsoRecord> UpdatedQsos { get; } = [];

        public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<ValidateSetupStepResponse> ValidateStepAsync(ValidateSetupStepRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(string username, string password, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(string apiKey, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default)
        {
            if (RefreshException is not null)
            {
                throw RefreshException;
            }

            return Task.FromResult(RecentQsos);
        }

        public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default)
        {
            UpdatedQsos.Add(qso.Clone());
            return Task.FromResult(new UpdateQsoResponse { Success = true });
        }

        public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) =>
            Task.FromResult(new SyncWithQrzResponse());

        public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetSyncStatusResponse());

        public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();
    }
}
