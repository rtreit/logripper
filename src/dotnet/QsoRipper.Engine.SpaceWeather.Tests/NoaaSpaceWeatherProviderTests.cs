#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
#pragma warning disable CA1307 // Use StringComparison for string comparison

using System.Net;
using QsoRipper.Domain;

namespace QsoRipper.Engine.SpaceWeather.Tests;

public sealed class NoaaSpaceWeatherProviderTests
{
    [Fact]
    public void FetchCurrent_ValidResponses_ReturnsValidSnapshot()
    {
        const string kpJson = """
            [{"time_tag":"2026-04-09T03:00:00","Kp":2.33,"a_running":9,"station_count":8}]
            """;
        const string solarText = "# header\n2026 04 12   99     47\n";

        using var handler = new MockHandler(request =>
        {
            var content = request.RequestUri!.AbsoluteUri.Contains("/kp")
                ? kpJson
                : solarText;
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(content),
            };
        });
        using var httpClient = new HttpClient(handler);
        var config = new NoaaSpaceWeatherConfig
        {
            KpIndexUrl = "http://test/kp",
            SolarIndicesUrl = "http://test/solar",
        };
        var provider = new NoaaSpaceWeatherProvider(httpClient, config);

        var snapshot = provider.FetchCurrent();

        Assert.Equal(SpaceWeatherStatus.Current, snapshot.Status);
        Assert.Equal(2.33, snapshot.PlanetaryKIndex, precision: 2);
        Assert.Equal(9u, snapshot.PlanetaryAIndex);
        Assert.Equal(99.0, snapshot.SolarFluxIndex, precision: 1);
        Assert.Equal(47u, snapshot.SunspotNumber);
        Assert.Equal(0u, snapshot.GeomagneticStormScale);
        Assert.Equal("NOAA SWPC", snapshot.SourceName);
    }

    [Fact]
    public void FetchCurrent_HttpFailure_ThrowsTransportError()
    {
        using var handler = new MockHandler(_ =>
            new HttpResponseMessage(HttpStatusCode.InternalServerError));
        using var httpClient = new HttpClient(handler);
        var config = new NoaaSpaceWeatherConfig
        {
            KpIndexUrl = "http://test/kp",
            SolarIndicesUrl = "http://test/solar",
        };
        var provider = new NoaaSpaceWeatherProvider(httpClient, config);

        var ex = Assert.Throws<SpaceWeatherProviderException>(provider.FetchCurrent);

        Assert.Equal(SpaceWeatherProviderErrorKind.Transport, ex.Kind);
    }

    [Fact]
    public void FetchCurrent_InvalidKpJson_ThrowsParseError()
    {
        using var handler = new MockHandler(request =>
        {
            var content = request.RequestUri!.AbsoluteUri.Contains("/kp")
                ? "not json"
                : "# header\n2026 04 12   99     47\n";
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(content),
            };
        });
        using var httpClient = new HttpClient(handler);
        var config = new NoaaSpaceWeatherConfig
        {
            KpIndexUrl = "http://test/kp",
            SolarIndicesUrl = "http://test/solar",
        };
        var provider = new NoaaSpaceWeatherProvider(httpClient, config);

        var ex = Assert.Throws<SpaceWeatherProviderException>(provider.FetchCurrent);

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
    }

    [Fact]
    public void FetchCurrent_HighKp_SetsStormScale()
    {
        const string kpJson = """
            [{"time_tag":"2026-04-09T03:00:00","Kp":7.5,"a_running":48,"station_count":8}]
            """;
        const string solarText = "2026 04 12   99     47\n";

        using var handler = new MockHandler(request =>
        {
            var content = request.RequestUri!.AbsoluteUri.Contains("/kp")
                ? kpJson
                : solarText;
            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(content),
            };
        });
        using var httpClient = new HttpClient(handler);
        var config = new NoaaSpaceWeatherConfig
        {
            KpIndexUrl = "http://test/kp",
            SolarIndicesUrl = "http://test/solar",
        };
        var provider = new NoaaSpaceWeatherProvider(httpClient, config);

        var snapshot = provider.FetchCurrent();

        Assert.Equal(3u, snapshot.GeomagneticStormScale); // G3 Strong
    }

    private sealed class MockHandler(Func<HttpRequestMessage, HttpResponseMessage> handler) : HttpMessageHandler
    {
        protected override HttpResponseMessage Send(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            return handler(request);
        }

        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            return Task.FromResult(handler(request));
        }
    }
}
