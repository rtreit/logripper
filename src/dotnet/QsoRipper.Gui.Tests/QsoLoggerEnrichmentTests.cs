using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

public sealed class QsoLoggerEnrichmentTests
{
    [Fact]
    public void EnrichFromLookupPopulatesAllAvailableFields()
    {
        var qso = new QsoRecord { WorkedCallsign = "N7DOE" };
        var record = new CallsignRecord
        {
            FirstName = "Harry",
            LastName = "Wong",
            GridSquare = "CN87",
            Country = "United States",
            DxccEntityId = 291,
            State = "WA",
            CqZone = 3,
            ItuZone = 7,
            County = "King",
            Iota = "NA-065",
            DxccContinent = "NA",
        };

        QsoLoggerViewModel.EnrichFromLookup(qso, record);

        Assert.Equal("Harry Wong", qso.WorkedOperatorName);
        Assert.Equal("CN87", qso.WorkedGrid);
        Assert.Equal("United States", qso.WorkedCountry);
        Assert.Equal(291u, qso.WorkedDxcc);
        Assert.Equal("WA", qso.WorkedState);
        Assert.Equal(3u, qso.WorkedCqZone);
        Assert.Equal(7u, qso.WorkedItuZone);
        Assert.Equal("King", qso.WorkedCounty);
        Assert.Equal("NA-065", qso.WorkedIota);
        Assert.Equal("NA", qso.WorkedContinent);
    }

    private sealed class CwSampleHarness : ICwWpmSampleSource
    {
        public bool IsRunning => false;
        public CwWpmSample? LatestSample { get; private set; }
        public CwLockState CurrentLockState { get; private set; } = CwLockState.Locked;
        public event EventHandler<CwWpmSample>? SampleReceived;
        public event EventHandler? StatusChanged;
#pragma warning disable CS0067 // unused in tests
        public event EventHandler<string>? RawLineReceived;
        public event EventHandler<CwLockState>? LockStateChanged;
#pragma warning restore CS0067
        public void Emit(CwWpmSample s)
        {
            LatestSample = s;
            SampleReceived?.Invoke(this, s);
        }
        public void Start(string? deviceOverride) => StatusChanged?.Invoke(this, EventArgs.Empty);
        public void MarkAnchorHeard() { }
        public void Stop() => StatusChanged?.Invoke(this, EventArgs.Empty);
        public void Dispose() { }
    }

    [Fact]
    public void EnrichFromCwDecoderFillsRxWpmForCwQsoWhenSamplesPresent()
    {
        using var src = new CwSampleHarness();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(60));

        var start = new DateTimeOffset(2024, 6, 1, 12, 0, 0, TimeSpan.Zero);
        var end = start.AddSeconds(20);

        src.Emit(new CwWpmSample(start, 18.0, Epoch: 1));
        src.Emit(new CwWpmSample(start.AddSeconds(10), 22.0, Epoch: 1));

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), agg);
        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Cw };

        logger.EnrichFromCwDecoder(qso, start, end);

        // Time-weighted: (18*10 + 22*10)/20 = 20 ⇒ rounds to 20.
        Assert.True(qso.HasCwDecodeRxWpm);
        Assert.Equal(20u, qso.CwDecodeRxWpm);
    }

    [Fact]
    public void EnrichFromCwDecoderSkipsNonCwQso()
    {
        using var src = new CwSampleHarness();
        using var agg = new CwQsoWpmAggregator(src);
        src.Emit(new CwWpmSample(DateTimeOffset.UtcNow, 25.0, Epoch: 1));

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), agg);
        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Ssb };

        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-30), DateTimeOffset.UtcNow);

        Assert.False(qso.HasCwDecodeRxWpm);
    }

    [Fact]
    public void EnrichFromCwDecoderIsNoOpWhenAggregatorMissing()
    {
        var logger = new QsoLoggerViewModel(new FakeEngineClient());
        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Cw };

        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-30), DateTimeOffset.UtcNow);

        Assert.False(qso.HasCwDecodeRxWpm);
    }

    [Fact]
    public void EnrichFromCwDecoderSkipsCwQsoWithoutSamples()
    {
        using var src = new CwSampleHarness();
        using var agg = new CwQsoWpmAggregator(src);

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), agg);
        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Cw };

        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-30), DateTimeOffset.UtcNow);

        Assert.False(qso.HasCwDecodeRxWpm);
    }

    [Fact]
    public void EnrichFromCwDecoderFillsTranscriptForCwQsoWhenFragmentsPresent()
    {
        using var src = new CwSampleHarness();
        using var transcriptAgg = new CwQsoTranscriptAggregator(src);

        var start = DateTimeOffset.UtcNow.AddSeconds(-2);

        transcriptAgg.IngestForTest("{\"type\":\"char\",\"ch\":\"C\"}");
        transcriptAgg.IngestForTest("{\"type\":\"char\",\"ch\":\"Q\"}");
        transcriptAgg.IngestForTest("{\"type\":\"word\"}");
        transcriptAgg.IngestForTest("{\"type\":\"char\",\"ch\":\"K\"}");

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), cwWpmAggregator: null);
        logger.AttachCwTranscriptAggregator(transcriptAgg);

        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Cw };

        logger.EnrichFromCwDecoder(qso, start, DateTimeOffset.UtcNow.AddSeconds(2));

        Assert.True(qso.HasCwDecodeTranscript);
        Assert.Equal("CQ K", qso.CwDecodeTranscript);
    }

    [Fact]
    public void EnrichFromCwDecoderPreservesOperatorTypedTranscript()
    {
        using var src = new CwSampleHarness();
        using var transcriptAgg = new CwQsoTranscriptAggregator(src);
        transcriptAgg.IngestForTest("{\"type\":\"char\",\"ch\":\"X\"}");

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), cwWpmAggregator: null);
        logger.AttachCwTranscriptAggregator(transcriptAgg);

        var qso = new QsoRecord
        {
            WorkedCallsign = "K7DOE",
            Mode = Mode.Cw,
            CwDecodeTranscript = "operator typed",
        };

        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-2), DateTimeOffset.UtcNow.AddSeconds(2));

        Assert.Equal("operator typed", qso.CwDecodeTranscript);
    }

    [Fact]
    public void EnrichFromCwDecoderClearsCwFieldsWhenModeIsNotCw()
    {
        using var src = new CwSampleHarness();
        using var wpm = new CwQsoWpmAggregator(src);
        using var transcriptAgg = new CwQsoTranscriptAggregator(src);
        src.Emit(new CwWpmSample(DateTimeOffset.UtcNow, 25.0, Epoch: 1));
        transcriptAgg.IngestForTest("{\"type\":\"char\",\"ch\":\"Y\"}");

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), wpm);
        logger.AttachCwTranscriptAggregator(transcriptAgg);

        var qso = new QsoRecord
        {
            WorkedCallsign = "K7DOE",
            Mode = Mode.Ssb,
            CwDecodeRxWpm = 30,
            CwDecodeTranscript = "stale auto-fill",
        };

        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-30), DateTimeOffset.UtcNow);

        Assert.False(qso.HasCwDecodeRxWpm);
        Assert.False(qso.HasCwDecodeTranscript);
    }

    [Fact]
    public void EnrichFromCwDecoderTranscriptIsNoOpWhenAggregatorMissing()
    {
        var logger = new QsoLoggerViewModel(new FakeEngineClient());
        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Cw };

        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-30), DateTimeOffset.UtcNow);

        Assert.False(qso.HasCwDecodeTranscript);
    }

    [Fact]
    public void EnrichFromCwDecoderFillsBothWpmAndTranscriptForCwQso()
    {
        using var src = new CwSampleHarness();
        using var wpm = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(60));
        using var transcriptAgg = new CwQsoTranscriptAggregator(src);

        var start = new DateTimeOffset(2024, 6, 1, 12, 0, 0, TimeSpan.Zero);
        var end = start.AddSeconds(20);

        src.Emit(new CwWpmSample(start, 24.0, Epoch: 1));
        src.Emit(new CwWpmSample(start.AddSeconds(10), 24.0, Epoch: 1));
        transcriptAgg.IngestForTest("{\"type\":\"char\",\"ch\":\"R\"}");

        var logger = new QsoLoggerViewModel(new FakeEngineClient(), wpm);
        logger.AttachCwTranscriptAggregator(transcriptAgg);

        var qso = new QsoRecord { WorkedCallsign = "K7DOE", Mode = Mode.Cw };
        logger.EnrichFromCwDecoder(qso, start, end);

        // wpm aggregator queries on a window in the past; transcript
        // aggregator timestamps fragments at ingest-time using
        // DateTimeOffset.UtcNow, so use a wide window for transcript here
        // by re-running with a wide window:
        logger.EnrichFromCwDecoder(qso, DateTimeOffset.UtcNow.AddSeconds(-5), DateTimeOffset.UtcNow.AddSeconds(5));

        Assert.True(qso.HasCwDecodeRxWpm);
        Assert.Equal(24u, qso.CwDecodeRxWpm);
        Assert.Equal("R", qso.CwDecodeTranscript);
    }

    [Fact]
    public void EnrichFromLookupWithNullRecordLeavesQsoUnchanged()
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW" };

        QsoLoggerViewModel.EnrichFromLookup(qso, null);

        Assert.False(qso.HasWorkedOperatorName);
        Assert.False(qso.HasWorkedGrid);
        Assert.False(qso.HasWorkedCountry);
    }

    [Fact]
    public void EnrichFromLookupWithPartialRecordSetsOnlyAvailableFields()
    {
        var qso = new QsoRecord { WorkedCallsign = "VK3ABC" };
        var record = new CallsignRecord
        {
            FirstName = "Jane",
            Country = "Australia",
        };

        QsoLoggerViewModel.EnrichFromLookup(qso, record);

        Assert.Equal("Jane", qso.WorkedOperatorName);
        Assert.Equal("Australia", qso.WorkedCountry);
        Assert.False(qso.HasWorkedGrid);
        Assert.False(qso.HasWorkedState);
        Assert.Equal(0u, qso.WorkedDxcc);
    }

    [Fact]
    public void AcceptLookupRecordUpdatesDisplayFieldsWhenCallsignMatches()
    {
        var engine = new FakeEngineClient();
        var logger = new QsoLoggerViewModel(engine);
        logger.Callsign = "KD9SU";

        var record = new CallsignRecord
        {
            Callsign = "KD9SU",
            FirstName = "Richard",
            LastName = "Smith",
            GridSquare = "EN52",
            Country = "United States",
        };

        logger.AcceptLookupRecord(record);

        Assert.Equal("Richard Smith", logger.LookupName);
        Assert.Equal("EN52", logger.LookupGrid);
        Assert.Equal("United States", logger.LookupCountry);
    }

    [Fact]
    public void AcceptLookupRecordIgnoresMismatchedCallsign()
    {
        var engine = new FakeEngineClient();
        var logger = new QsoLoggerViewModel(engine);
        logger.Callsign = "W1AW";

        var record = new CallsignRecord
        {
            Callsign = "KD9SU",
            FirstName = "Richard",
            GridSquare = "EN52",
        };

        logger.AcceptLookupRecord(record);

        Assert.Equal(string.Empty, logger.LookupName);
        Assert.Equal(string.Empty, logger.LookupGrid);
    }

    [Fact]
    public async Task LogQsoAsyncIncludesNotesAndContestFields()
    {
        var engine = new CapturingEngineClient();
        var logger = new QsoLoggerViewModel(engine);
        logger.Callsign = "W1AW";
        logger.Notes = "Worked on 20m dipole";
        logger.ContestId = "CQWW-CW";
        logger.ExchangeSent = "599 05";
        logger.Comment = "Strong signal";

        await logger.LogQsoCommand.ExecuteAsync(null);

        Assert.NotNull(engine.LastLoggedQso);
        Assert.Equal("W1AW", engine.LastLoggedQso!.WorkedCallsign);
        Assert.Equal("Worked on 20m dipole", engine.LastLoggedQso.Notes);
        Assert.Equal("CQWW-CW", engine.LastLoggedQso.ContestId);
        Assert.Equal("599 05", engine.LastLoggedQso.ExchangeSent);
        Assert.Equal("Strong signal", engine.LastLoggedQso.Comment);
    }

    [Fact]
    public async Task LogQsoAsyncOmitsBlankNotesAndContestFields()
    {
        var engine = new CapturingEngineClient();
        var logger = new QsoLoggerViewModel(engine);
        logger.Callsign = "VK3ABC";
        // Leave Notes, ContestId, ExchangeSent at their defaults (empty)

        await logger.LogQsoCommand.ExecuteAsync(null);

        Assert.NotNull(engine.LastLoggedQso);
        Assert.False(engine.LastLoggedQso!.HasNotes);
        Assert.False(engine.LastLoggedQso.HasContestId);
        Assert.False(engine.LastLoggedQso.HasExchangeSent);
    }

    [Fact]
    public void CallsignSetterNormalizesTypedInputToUppercase()
    {
        var engine = new FakeEngineClient();
        var logger = new QsoLoggerViewModel(engine);

        logger.Callsign = "w1aw/p";

        Assert.Equal("W1AW/P", logger.Callsign);
        Assert.True(logger.IsLogEnabled);
    }

    [Fact]
    public async Task LogQsoCommandLeavesUtcEndTimestampNullWithoutF7()
    {
        var engine = new FakeEngineClient();
        var logger = new QsoLoggerViewModel(engine)
        {
            Callsign = "KW5CW",
        };

        await logger.LogQsoCommand.ExecuteAsync(null);

        Assert.NotNull(engine.LastLoggedQso);
        Assert.NotNull(engine.LastLoggedQso!.UtcTimestamp);
        // The duration timer is no longer auto-started — operator must press
        // F7 to acknowledge a real QSO. Without F7, no end timestamp is set.
        Assert.Null(engine.LastLoggedQso.UtcEndTimestamp);
    }

    [Fact]
    public async Task LogQsoCommandPopulatesUtcEndTimestampAfterF7()
    {
        var engine = new FakeEngineClient();
        var logger = new QsoLoggerViewModel(engine)
        {
            Callsign = "KW5CW",
        };

        // F7: operator acknowledges the QSO is underway, starting the
        // duration timer.
        await logger.AcknowledgeQsoStartCommand.ExecuteAsync(null);
        await logger.LogQsoCommand.ExecuteAsync(null);

        Assert.NotNull(engine.LastLoggedQso);
        Assert.NotNull(engine.LastLoggedQso!.UtcTimestamp);
        Assert.NotNull(engine.LastLoggedQso.UtcEndTimestamp);
        Assert.True(
            engine.LastLoggedQso.UtcEndTimestamp.ToDateTimeOffset()
            >= engine.LastLoggedQso.UtcTimestamp.ToDateTimeOffset());
    }

    private sealed class FakeEngineClient : IEngineClient
    {
        public QsoRecord? LastLoggedQso { get; private set; }

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

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<QsoRecord>>([]);

        public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

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
            throw new NotImplementedException();

        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<ComputeGreatCircleResponse> ComputeGreatCircleAsync(ComputeGreatCircleRequest request, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<GetActiveStationContextResponse> GetActiveStationContextAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<PurgeDeletedQsosResponse> PurgeDeletedQsosAsync(IReadOnlyList<string>? localIds = null, Timestamp? olderThan = null, bool includePendingRemoteDeletes = false, CancellationToken ct = default) => throw new NotImplementedException();
    }

    private sealed class CapturingEngineClient : IEngineClient
    {
        public QsoRecord? LastLoggedQso { get; private set; }

        public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default)
        {
            LastLoggedQso = qso;
            return Task.FromResult(new LogQsoResponse { LocalId = "test-id" });
        }

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

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<QsoRecord>>([]);

        public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<ComputeGreatCircleResponse> ComputeGreatCircleAsync(ComputeGreatCircleRequest request, CancellationToken ct = default) => throw new NotImplementedException();
        public Task<GetActiveStationContextResponse> GetActiveStationContextAsync(CancellationToken ct = default) => throw new NotImplementedException();
        public Task<PurgeDeletedQsosResponse> PurgeDeletedQsosAsync(IReadOnlyList<string>? localIds = null, Timestamp? olderThan = null, bool includePendingRemoteDeletes = false, CancellationToken ct = default) => throw new NotImplementedException();
    }
}
