using LogRipper.Domain;
using LogRipper.DebugHost.Models;
using LogRipper.DebugHost.Services;

namespace LogRipper.DebugHost.Tests;

public class ProtoJsonServiceTests
{
    [Fact]
    public void Describe_returns_json_and_binary_metadata()
    {
        var factory = new SampleProtoFactory();
        var service = new ProtoJsonService();

        var payload = service.Describe(factory.CreateLookupRequest("K7DBG", skipCache: true));

        Assert.Contains("\"callsign\": \"K7DBG\"", payload.Json);
        Assert.Contains("\"skipCache\": true", payload.Json);
        Assert.True(payload.ByteCount > 0);
        Assert.False(string.IsNullOrWhiteSpace(payload.Base64));
    }

    [Fact]
    public void Describe_supports_cw_qso_payloads()
    {
        var factory = new SampleProtoFactory();
        var service = new ProtoJsonService();
        var message = factory.CreateQsoRecord("K7DBG", new QsoSampleOptions
        {
            Band = Band._20M,
            Mode = Mode.Cw
        });

        var payload = service.Describe(message);

        Assert.Contains("\"band\":", payload.Json);
        Assert.Contains("\"mode\":", payload.Json);
        Assert.Contains("\"raw\": \"599\"", payload.Json);
        Assert.Contains("\"tone\": 9", payload.Json);
        Assert.DoesNotContain("\"submode\":", payload.Json);
    }
}
