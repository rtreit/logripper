#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
#pragma warning disable CA1307 // Use StringComparison for string comparison

namespace QsoRipper.Engine.SpaceWeather.Tests;

public sealed class NoaaKIndexParsingTests
{
    [Fact]
    public void ParseKpIndexJson_ValidJson_ReturnsLastEntry()
    {
        var json = """
            [
                {"time_tag":"2026-04-09T00:00:00","Kp":1.67,"a_running":6,"station_count":8},
                {"time_tag":"2026-04-09T03:00:00","Kp":2.33,"a_running":9,"station_count":8}
            ]
            """;

        var entry = NoaaDataParsers.ParseKpIndexJson(json);

        Assert.Equal("2026-04-09T03:00:00", entry.TimeTag);
        Assert.Equal(2.33, entry.Kp, precision: 2);
        Assert.Equal(9u, entry.ARunning);
    }

    [Fact]
    public void ParseKpIndexJson_SingleEntry_ReturnsIt()
    {
        var json = """[{"time_tag":"2026-04-09T00:00:00","Kp":5.0,"a_running":48,"station_count":8}]""";

        var entry = NoaaDataParsers.ParseKpIndexJson(json);

        Assert.Equal("2026-04-09T00:00:00", entry.TimeTag);
        Assert.Equal(5.0, entry.Kp, precision: 2);
        Assert.Equal(48u, entry.ARunning);
    }

    [Fact]
    public void ParseKpIndexJson_EmptyArray_ThrowsParseError()
    {
        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseKpIndexJson("[]"));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
        Assert.Contains("no entries", ex.Message);
    }

    [Fact]
    public void ParseKpIndexJson_InvalidJson_ThrowsParseError()
    {
        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseKpIndexJson("not json at all"));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
    }

    [Fact]
    public void ParseKpIndexJson_MissingKpField_ThrowsParseError()
    {
        var json = """[{"time_tag":"2026-04-09T00:00:00","a_running":6}]""";

        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseKpIndexJson(json));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
    }

    [Fact]
    public void ParseTimestamp_ValidKIndexTimestamp_ParsesCorrectly()
    {
        var timestamp = NoaaDataParsers.ParseTimestamp("2026-04-13T18:00:00");

        Assert.Equal(1_776_103_200, timestamp.Seconds);
    }

    [Fact]
    public void ParseTimestamp_InvalidFormat_ThrowsParseError()
    {
        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseTimestamp("not-a-date"));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
    }
}
