using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

public sealed class FullQsoCardViewModelTests
{
    [Fact]
    public async Task SaveCommandLogsNewQsoFromLoggerSeed()
    {
        var engine = new RecordingEngineClient();
        var logger = new QsoLoggerViewModel(engine)
        {
            Callsign = "kd9su",
            FrequencyMhz = "14.074",
            Notes = "Portable op",
            Comment = "Loud signal",
            ContestId = "ARRL-FD",
            ExchangeSent = "1D OR",
        };

        logger.AcceptLookupRecord(new CallsignRecord
        {
            Callsign = "KD9SU",
            FirstName = "Richard",
            LastName = "Smith",
            GridSquare = "EN52",
            DxccCountryName = "United States",
            State = "IL",
        });

        var card = FullQsoCardViewModel.ForNew(engine, logger);
        card.StationCallsign = "K7RND";
        card.SelectedBand = "20M";
        card.SelectedMode = "CW";
        card.RstSent = "599";
        card.RstReceived = "579";
        card.ExtraFieldsText = "MYFLAG=Y";

        await card.SaveCommand.ExecuteAsync(null);

        Assert.NotNull(engine.LastLoggedQso);
        var qso = engine.LastLoggedQso!;
        Assert.Equal("KD9SU", qso.WorkedCallsign);
        Assert.Equal("K7RND", qso.StationCallsign);
        Assert.Equal(Band._20M, qso.Band);
        Assert.Equal(Mode.Cw, qso.Mode);
        Assert.Equal("Richard Smith", qso.WorkedOperatorName);
        Assert.Equal("EN52", qso.WorkedGrid);
        Assert.Equal("United States", qso.WorkedCountry);
        Assert.Equal("1D OR", qso.ExchangeSent);
        Assert.Equal("Y", qso.ExtraFields["MYFLAG"]);
        Assert.Equal("logged-qso", card.LocalId);
        Assert.Equal(string.Empty, logger.Callsign);
        Assert.Equal("Logged KD9SU.", logger.LogStatusText);
    }

    [Fact]
    public void ForNewSeedsWorkedOperatorCallsignFromLoggerCallsign()
    {
        var engine = new RecordingEngineClient();
        var logger = new QsoLoggerViewModel(engine)
        {
            Callsign = "n0call",
        };

        var card = FullQsoCardViewModel.ForNew(engine, logger);

        Assert.Equal("N0CALL", card.WorkedOperatorCallsign);
    }

    [Fact]
    public void ForNewMapsLoggerBandAndModeToCardOptions()
    {
        var engine = new RecordingEngineClient();
        var logger = new QsoLoggerViewModel(engine)
        {
            SelectedBandIndex = OperatorOptions.FindBandIndex(Band._40M),
            SelectedModeIndex = OperatorOptions.FindModeIndex(Mode.Mfsk, "FT4"),
        };

        var card = FullQsoCardViewModel.ForNew(engine, logger);

        Assert.Equal("40M", card.SelectedBand);
        Assert.Contains(card.SelectedBand, card.BandOptions);
        Assert.Equal("MFSK", card.SelectedMode);
        Assert.Contains(card.SelectedMode, card.ModeOptions);
        Assert.Equal("FT4", card.Submode);
    }

    [Fact]
    public void WorkedCallsignUpdatesWorkedOperatorCallsignWhileAutoSeeded()
    {
        var engine = new RecordingEngineClient();
        var logger = new QsoLoggerViewModel(engine);
        var card = FullQsoCardViewModel.ForNew(engine, logger);

        card.WorkedCallsign = "k9xyz";

        Assert.Equal("K9XYZ", card.WorkedCallsign);
        Assert.Equal("K9XYZ", card.WorkedOperatorCallsign);
    }

    [Fact]
    public void ForNewSeedsStationSnapshotFromActiveProfile()
    {
        var engine = new RecordingEngineClient();
        var logger = new QsoLoggerViewModel(engine);
        var profile = new StationProfile
        {
            StationCallsign = "K7RND",
            OperatorCallsign = "N7OP",
            Grid = "CN85",
            Country = "United States",
            ArrlSection = "WWA",
            CqZone = 3,
        };

        var card = FullQsoCardViewModel.ForNew(engine, logger, profile);

        Assert.Equal("K7RND", card.StationCallsign);
        Assert.Equal("K7RND", card.SnapshotStationCallsign);
        Assert.Equal("N7OP", card.SnapshotOperatorCallsign);
        Assert.Equal("CN85", card.SnapshotGrid);
        Assert.Equal("United States", card.SnapshotCountry);
        Assert.Equal("WWA", card.SnapshotArrlSection);
        Assert.Equal("3", card.SnapshotCqZone);
        Assert.True(card.ShowAdvancedStationFields);
    }

    [Fact]
    public async Task SaveCommandUpdatesExistingQsoAndClearsOptionalFields()
    {
        var engine = new RecordingEngineClient();
        var existing = new QsoRecord
        {
            LocalId = "qso-1",
            WorkedCallsign = "W1AW",
            StationCallsign = "K7RND",
            UtcTimestamp = Timestamp.FromDateTimeOffset(new DateTimeOffset(2026, 4, 19, 22, 15, 0, TimeSpan.Zero)),
            Band = Band._20M,
            Mode = Mode.Ssb,
            Notes = "Old note",
            Comment = "Old comment",
            QslSentStatus = QslStatus.No,
            SyncStatus = SyncStatus.Modified,
        };
        existing.ExtraFields["OLD"] = "1";

        var card = FullQsoCardViewModel.ForEdit(engine, existing);
        card.SelectedMode = "CW";
        card.RstSent = "599";
        card.RstReceived = "589";
        card.SelectedQslSentStatus = "Yes";
        card.QslSentDateText = "2026-04-19";
        card.Notes = string.Empty;
        card.Comment = "Updated";
        card.ExtraFieldsText = "OLD=2\nNEW=3";

        await card.SaveCommand.ExecuteAsync(null);

        Assert.NotNull(engine.LastUpdatedQso);
        var updated = engine.LastUpdatedQso!;
        Assert.Equal("qso-1", updated.LocalId);
        Assert.Equal(Mode.Cw, updated.Mode);
        Assert.Equal(QslStatus.Yes, updated.QslSentStatus);
        Assert.NotNull(updated.QslSentDate);
        Assert.False(updated.HasNotes);
        Assert.Equal("Updated", updated.Comment);
        Assert.Equal("2", updated.ExtraFields["OLD"]);
        Assert.Equal("3", updated.ExtraFields["NEW"]);
    }

    [Fact]
    public async Task OpenQsoCardCommandUsesSelectedGridQsoForEdit()
    {
        var engine = new RecordingEngineClient
        {
            RecentQsos =
            [
                new QsoRecord
                {
                    LocalId = "grid-qso",
                    WorkedCallsign = "N0CALL",
                    StationCallsign = "K7RND",
                    UtcTimestamp = Timestamp.FromDateTimeOffset(new DateTimeOffset(2026, 4, 19, 23, 0, 0, TimeSpan.Zero)),
                    Band = Band._40M,
                    Mode = Mode.Cw,
                },
            ],
        };

        using var viewModel = new MainWindowViewModel(engine);
        await viewModel.RecentQsos.RefreshAsync();
        viewModel.FocusGridCommand.Execute(null);

        viewModel.OpenQsoCardCommand.Execute(null);

        Assert.NotNull(viewModel.FullQsoCard);
        var card = viewModel.FullQsoCard!;
        Assert.True(card.IsEditingExisting);
        Assert.Equal("N0CALL", card.WorkedCallsign);
    }

    private sealed class RecordingEngineClient : IEngineClient
    {
        public QsoRecord? LastLoggedQso { get; private set; }

        public QsoRecord? LastUpdatedQso { get; private set; }

        public IReadOnlyList<QsoRecord> RecentQsos { get; init; } = [];

        public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetSetupWizardStateResponse());

        public Task<ValidateSetupStepResponse> ValidateStepAsync(ValidateSetupStepRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(string username, string password, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetSetupStatusResponse
            {
                Status = new SetupStatus
                {
                    SetupComplete = true,
                    IsFirstRun = false,
                },
            });

        public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(string apiKey, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) =>
            Task.FromResult(RecentQsos);

        public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default)
        {
            LastUpdatedQso = qso.Clone();
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

        public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default)
        {
            LastLoggedQso = qso.Clone();
            return Task.FromResult(new LogQsoResponse { LocalId = "logged-qso" });
        }

        public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetRigSnapshotResponse());

        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetRigStatusResponse());

        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) =>
            Task.FromResult(new GetCurrentSpaceWeatherResponse());
    }
}
