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
    public async Task RefreshAsyncLoadsRecentQsosAndSelectsFirstItem()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW", Band._40M, Mode.Cw, 7025, "CN87", "First recent QSO", operatorName: "Alice", state: "WA", country: "United States", utcTimestamp: new DateTimeOffset(2026, 4, 13, 22, 16, 0, TimeSpan.Zero)),
                CreateQso("qso-2", "K7RND", Band._20M, Mode.Ft8, 14074, "CN88", "Second recent QSO", operatorName: "Bob", state: "BC", country: "Canada", utcTimestamp: new DateTimeOffset(2026, 4, 13, 22, 15, 0, TimeSpan.Zero))
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
        Assert.Equal("Sorted by UTC time (descending)", viewModel.SortStatusText);
        Assert.Equal("Showing 2 recent QSOs", viewModel.SummaryText);
    }

    [Fact]
    public async Task SearchTextFiltersAcrossMultipleColumnsAsUserTypes()
    {
        var engine = new FakeEngineClient
        {
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW", Band._20M, Mode.Ft8, 14074, "CN87", "CQ test", operatorName: "Alice", state: "WA", country: "United States"),
                CreateQso("qso-2", "VE7ABC", Band._40M, Mode.Ssb, 7185, "CN88", "Morning ragchew", operatorName: "Bob", state: "BC", country: "Canada")
            ]
        };

        var viewModel = new RecentQsoListViewModel(engine);
        await viewModel.RefreshAsync();

        viewModel.SearchText = "w1aw united 57";

        Assert.Single(viewModel.VisibleItems);
        Assert.Equal("qso-1", viewModel.VisibleItems[0].LocalId);
        Assert.Equal("Showing 1 of 2 recent QSOs", viewModel.SummaryText);
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
    public void MatchesSearchUsesAllTokensAgainstFormattedColumns()
    {
        var item = RecentQsoItemViewModel.FromQso(
            CreateQso("qso-1", "W1AW", Band._40M, Mode.Cw, 7025, "CN87", "Evening CW", operatorName: "Alice", state: "WA", country: "United States"));

        Assert.True(RecentQsoListViewModel.MatchesSearch(item, "alice 40m united"));
        Assert.False(RecentQsoListViewModel.MatchesSearch(item, "alice 40m canada"));
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
        Assert.Equal("Call ↑", viewModel.CallsignHeaderText);

        viewModel.ApplySort(RecentQsoSortColumn.Callsign);
        Assert.Equal("W1AW", viewModel.VisibleItems[0].WorkedCallsign);
        Assert.Equal("Call ↓", viewModel.CallsignHeaderText);
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
        DateTimeOffset? utcTimestamp = null)
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
            WorkedOperatorName = operatorName ?? string.Empty,
            WorkedState = state ?? string.Empty,
            WorkedCountry = country ?? string.Empty,
            RstSent = new RstReport { Raw = "59" },
            RstReceived = new RstReport { Raw = "57" },
            SyncStatus = SyncStatus.LocalOnly
        };
    }

    private sealed class FakeEngineClient : IEngineClient
    {
        public IReadOnlyList<QsoRecord> RecentQsos { get; set; } = [];

        public RpcException? RefreshException { get; set; }

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

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default)
        {
            if (RefreshException is not null)
            {
                throw RefreshException;
            }

            return Task.FromResult(RecentQsos);
        }
    }
}
