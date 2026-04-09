using LogRipper.Domain;
using LogRipper.DebugHost.Models;
using LogRipper.DebugHost.Services;

namespace LogRipper.DebugHost.Tests;

public class SampleProtoFactoryTests
{
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

        Assert.Equal("W1AW", result.QueriedCallsign);
        Assert.NotNull(result.Record);
        Assert.Equal("W1AW", result.Record.Callsign);
        Assert.True(result.CacheHit);
    }

    [Theory]
    [InlineData(SampleMessageType.LookupRequest)]
    [InlineData(SampleMessageType.LookupResult)]
    [InlineData(SampleMessageType.CallsignRecord)]
    [InlineData(SampleMessageType.QsoRecord)]
    [InlineData(SampleMessageType.DxccEntity)]
    public void Sample_message_generation_supports_all_registered_types(SampleMessageType sampleType)
    {
        var message = _factory.CreateSampleMessage(sampleType, "AA7BQ");

        Assert.NotNull(message);
        Assert.True(message.CalculateSize() > 0);
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
}
