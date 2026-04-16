using QsoRipper.Domain;

namespace QsoRipper.Engine.SpaceWeather;

/// <summary>
/// NOAA SWPC space weather data provider.
/// Fetches planetary K-index and daily solar indices from NOAA endpoints.
/// </summary>
public sealed class NoaaSpaceWeatherProvider : ISpaceWeatherProvider
{
    private readonly HttpClient _httpClient;
    private readonly string _kpIndexUrl;
    private readonly string _solarIndicesUrl;

    public NoaaSpaceWeatherProvider(HttpClient httpClient, NoaaSpaceWeatherConfig config)
    {
        ArgumentNullException.ThrowIfNull(httpClient);
        ArgumentNullException.ThrowIfNull(config);

        _httpClient = httpClient;
        _kpIndexUrl = config.KpIndexUrl;
        _solarIndicesUrl = config.SolarIndicesUrl;
    }

    /// <inheritdoc/>
    public SpaceWeatherSnapshot FetchCurrent()
    {
        var kpEntry = FetchKpIndex();
        var solarEntry = FetchSolarIndices();

        return new SpaceWeatherSnapshot
        {
            ObservedAt = NoaaDataParsers.ParseTimestamp(kpEntry.TimeTag),
            Status = SpaceWeatherStatus.Current,
            PlanetaryKIndex = kpEntry.Kp,
            PlanetaryAIndex = kpEntry.ARunning,
            SolarFluxIndex = solarEntry.SolarFlux,
            SunspotNumber = solarEntry.SunspotNumber,
            GeomagneticStormScale = NoaaDataParsers.CalculateGeomagneticStormScale(kpEntry.Kp),
            SourceName = "NOAA SWPC",
        };
    }

    private KpIndexEntry FetchKpIndex()
    {
        string body;
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, _kpIndexUrl);
            using var response = _httpClient.Send(request, HttpCompletionOption.ResponseContentRead);
            response.EnsureSuccessStatusCode();
            using var stream = response.Content.ReadAsStream();
            using var reader = new StreamReader(stream);
            body = reader.ReadToEnd();
        }
        catch (HttpRequestException ex)
        {
            throw SpaceWeatherProviderException.Transport(
                $"Failed to fetch NOAA planetary K-index data: {ex.Message}");
        }
        catch (TaskCanceledException ex)
        {
            throw SpaceWeatherProviderException.Transport(
                $"NOAA planetary K-index request timed out: {ex.Message}");
        }

        return NoaaDataParsers.ParseKpIndexJson(body);
    }

    private SolarIndicesEntry FetchSolarIndices()
    {
        string body;
        try
        {
            using var request = new HttpRequestMessage(HttpMethod.Get, _solarIndicesUrl);
            using var response = _httpClient.Send(request, HttpCompletionOption.ResponseContentRead);
            response.EnsureSuccessStatusCode();
            using var stream = response.Content.ReadAsStream();
            using var reader = new StreamReader(stream);
            body = reader.ReadToEnd();
        }
        catch (HttpRequestException ex)
        {
            throw SpaceWeatherProviderException.Transport(
                $"Failed to fetch NOAA daily solar indices: {ex.Message}");
        }
        catch (TaskCanceledException ex)
        {
            throw SpaceWeatherProviderException.Transport(
                $"NOAA daily solar indices request timed out: {ex.Message}");
        }

        return NoaaDataParsers.ParseSolarIndicesText(body);
    }
}
