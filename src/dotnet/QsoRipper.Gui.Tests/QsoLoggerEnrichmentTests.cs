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
    public void CallsignSetterNormalizesTypedInputToUppercase()
    {
        var engine = new FakeEngineClient();
        var logger = new QsoLoggerViewModel(engine);

        logger.Callsign = "w1aw/p";

        Assert.Equal("W1AW/P", logger.Callsign);
        Assert.True(logger.IsLogEnabled);
    }

    private sealed class FakeEngineClient : IEngineClient
    {
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

        public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();
    }
}
