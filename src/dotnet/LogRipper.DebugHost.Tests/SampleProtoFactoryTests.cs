using LogRipper.DebugHost.Models;
using LogRipper.DebugHost.Services;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class SampleProtoFactoryTests
{
    private readonly ProtoSampleCatalog _catalog = new();
    private readonly SampleProtoFactory _factory = new();

    [Fact]
    public void Lookup_request_normalizes_callsigns()
    {
        var request = _factory.CreateLookupRequest("  k7dbg  ", skipCache: true);

        Assert.Equal("K7DBG", request.Callsign);
        Assert.True(request.SkipCache);
    }

    [Fact]
    public void Lookup_result_contains_requested_callsign_and_record()
    {
        var result = _factory.CreateLookupResult("w1aw");

        Assert.Equal(LookupState.Found, result.State);
        Assert.Equal("W1AW", result.QueriedCallsign);
        Assert.NotNull(result.Record);
        Assert.Equal("W1AW", result.Record.Callsign);
        Assert.True(result.CacheHit);
    }

    [Fact]
    public void Sample_message_generation_supports_all_registered_types()
    {
        foreach (var sampleDefinition in _catalog.GetDefinitions())
        {
            var message = _factory.CreateSampleMessage(sampleDefinition.MessageType, "AA7BQ");

            Assert.NotNull(message);
            Assert.IsType(sampleDefinition.MessageType, message);
        }
    }

    [Fact]
    public void Messages_with_fields_generate_non_default_wire_payloads()
    {
        foreach (var sampleDefinition in _catalog.GetDefinitions())
        {
            var message = _factory.CreateSampleMessage(sampleDefinition.MessageType, "AA7BQ");

            if (message.Descriptor.Fields.InFieldNumberOrder().Count == 0)
            {
                Assert.Equal(0, message.CalculateSize());
                continue;
            }

            Assert.True(
                message.CalculateSize() > 0,
                $"{sampleDefinition.Label} should serialize at least one populated field.");
        }
    }

    [Fact]
    public void Save_station_profile_request_includes_nested_profile_payload()
    {
        var request = Assert.IsType<SaveStationProfileRequest>(
            _factory.CreateSampleMessage(typeof(SaveStationProfileRequest), "AA7BQ"));

        Assert.False(string.IsNullOrWhiteSpace(request.ProfileId));
        Assert.NotNull(request.Profile);
        Assert.False(string.IsNullOrWhiteSpace(request.Profile.ProfileName));
        Assert.False(string.IsNullOrWhiteSpace(request.Profile.StationCallsign));
    }

    [Fact]
    public void Save_station_profile_request_changes_between_generations()
    {
        var first = Assert.IsType<SaveStationProfileRequest>(
            _factory.CreateSampleMessage(typeof(SaveStationProfileRequest), "AA7BQ"));
        var second = Assert.IsType<SaveStationProfileRequest>(
            _factory.CreateSampleMessage(typeof(SaveStationProfileRequest), "AA7BQ"));

        Assert.NotEqual(first.ProfileId, second.ProfileId);
    }

    [Fact]
    public void Sync_request_generation_sets_full_sync()
    {
        var request = Assert.IsType<SyncWithQrzRequest>(
            _factory.CreateSampleMessage(typeof(SyncWithQrzRequest), "AA7BQ"));

        Assert.True(request.FullSync);
        Assert.True(request.CalculateSize() > 0);
    }

    [Fact]
    public void Qso_record_generation_supports_cw_without_submode()
    {
        var record = _factory.CreateQsoRecord("w1aw", new QsoSampleOptions
        {
            Band = Band._20M,
            Mode = Mode.Cw
        });

        Assert.Equal(Band._20M, record.Band);
        Assert.Equal(Mode.Cw, record.Mode);
        Assert.False(record.HasSubmode);
        Assert.Equal("599", record.RstSent.Raw);
        Assert.Equal((uint)9, record.RstSent.Tone);
        Assert.Equal("579", record.RstReceived.Raw);
        Assert.NotNull(record.StationSnapshot);
        Assert.Equal("K7DBG", record.StationSnapshot.StationCallsign);
        Assert.Equal("CN87up", record.StationSnapshot.Grid);
    }

    [Fact]
    public void Qso_record_generation_assigns_expected_ssb_submode()
    {
        var record = _factory.CreateQsoRecord("w1aw", new QsoSampleOptions
        {
            Band = Band._20M,
            Mode = Mode.Ssb
        });

        Assert.True(record.HasSubmode);
        Assert.Equal("USB", record.Submode);
        Assert.Equal("59", record.RstSent.Raw);
    }

    [Fact]
    public void Station_profile_generation_populates_sample_metadata()
    {
        var profile = _factory.CreateStationProfile();

        Assert.Equal("Home", profile.ProfileName);
        Assert.Equal("K7DBG", profile.StationCallsign);
        Assert.Equal("CN87up", profile.Grid);
        Assert.Equal((uint)291, profile.Dxcc);
    }

    [Fact]
    public void Station_snapshot_generation_populates_sample_metadata()
    {
        var snapshot = _factory.CreateStationSnapshot();

        Assert.Equal("Home", snapshot.ProfileName);
        Assert.Equal("K7DBG", snapshot.StationCallsign);
        Assert.Equal("KING", snapshot.County);
        Assert.Equal((uint)6, snapshot.ItuZone);
    }
}
#pragma warning restore CA1707
