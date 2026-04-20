using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

public sealed class MainWindowViewModelTests
{
    [Fact]
    public void FocusLoggerCommandDoesNotRequestGridFocus()
    {
        using var viewModel = new MainWindowViewModel(new FakeEngineClient());
        var loggerFocusRequests = 0;
        var gridFocusRequests = 0;
        viewModel.LoggerFocusRequested += (_, _) => loggerFocusRequests++;
        viewModel.GridFocusRequested += (_, _) => gridFocusRequests++;

        viewModel.FocusLoggerCommand.Execute(null);

        Assert.Equal(1, loggerFocusRequests);
        Assert.Equal(0, gridFocusRequests);
    }

    [Fact]
    public void FocusSearchCommandDoesNotRequestGridFocus()
    {
        using var viewModel = new MainWindowViewModel(new FakeEngineClient());
        var searchFocusRequests = 0;
        var gridFocusRequests = 0;
        viewModel.SearchFocusRequested += (_, _) => searchFocusRequests++;
        viewModel.GridFocusRequested += (_, _) => gridFocusRequests++;

        viewModel.FocusSearchCommand.Execute(null);

        Assert.Equal(1, searchFocusRequests);
        Assert.Equal(0, gridFocusRequests);
    }

    [Fact]
    public async Task CheckFirstRunAsyncCompletesBeforeSlowSyncStatusFinishes()
    {
        var syncStatusSource = new TaskCompletionSource<GetSyncStatusResponse>(TaskCreationOptions.RunContinuationsAsynchronously);
        var engine = new FakeEngineClient
        {
            SetupStatus = new GetSetupStatusResponse
            {
                Status = new SetupStatus
                {
                    SetupComplete = true,
                    IsFirstRun = false,
                },
            },
            RecentQsos =
            [
                CreateQso("qso-1", "W1AW"),
            ],
            SyncStatusTask = syncStatusSource.Task,
        };

        using var viewModel = new MainWindowViewModel(engine);

        await viewModel.CheckFirstRunAsync();

        Assert.True(viewModel.RecentQsos.HasLoaded);
        Assert.Equal("Ready", viewModel.StatusMessage);
        Assert.Equal("Sync: never", viewModel.SyncStatusText);

        syncStatusSource.SetResult(new GetSyncStatusResponse
        {
            LastSync = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow - TimeSpan.FromMinutes(3)),
        });

        await WaitUntilAsync(
            () => viewModel.SyncStatusText.Contains("Last sync", StringComparison.Ordinal),
            TimeSpan.FromSeconds(1));

        Assert.Contains("Last sync", viewModel.SyncStatusText, StringComparison.Ordinal);
    }

    private static async Task WaitUntilAsync(Func<bool> predicate, TimeSpan timeout)
    {
        var deadline = DateTime.UtcNow + timeout;
        while (DateTime.UtcNow < deadline)
        {
            if (predicate())
            {
                return;
            }

            await Task.Delay(10);
        }

        Assert.True(predicate(), "Condition was not satisfied before the timeout elapsed.");
    }

    private static QsoRecord CreateQso(string localId, string workedCallsign)
    {
        return new QsoRecord
        {
            LocalId = localId,
            WorkedCallsign = workedCallsign,
            StationCallsign = "K7RND",
            UtcTimestamp = Timestamp.FromDateTimeOffset(new DateTimeOffset(2026, 4, 13, 22, 15, 0, TimeSpan.Zero)),
            Band = Band._20M,
            Mode = Mode.Cw,
            FrequencyKhz = 14025,
            WorkedGrid = "CN87",
            Comment = "Loaded",
            Notes = string.Empty,
            WorkedCountry = "United States",
            WorkedOperatorName = "Alice",
            WorkedState = "WA",
            RstSent = new RstReport { Raw = "59" },
            RstReceived = new RstReport { Raw = "57" },
        };
    }

    private sealed class FakeEngineClient : IEngineClient
    {
        public GetSetupStatusResponse SetupStatus { get; init; } = new();

        public IReadOnlyList<QsoRecord> RecentQsos { get; init; } = [];

        public Task<GetSyncStatusResponse> SyncStatusTask { get; init; } = Task.FromResult(new GetSyncStatusResponse());

        public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetSetupWizardStateResponse { Status = SetupStatus.Status ?? new SetupStatus() });

        public Task<ValidateSetupStepResponse> ValidateStepAsync(ValidateSetupStepRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(string username, string password, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) =>
            Task.FromResult(SetupStatus);

        public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(string apiKey, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) =>
            Task.FromResult(RecentQsos);

        public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) =>
            Task.FromResult(new SyncWithQrzResponse());

        public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) =>
            SyncStatusTask;

        public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetRigSnapshotResponse());

        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetRigStatusResponse());

        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetCurrentSpaceWeatherResponse());
    }
}
