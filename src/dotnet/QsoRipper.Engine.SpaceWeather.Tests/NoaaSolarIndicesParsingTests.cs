#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
#pragma warning disable CA1307 // Use StringComparison for string comparison

namespace QsoRipper.Engine.SpaceWeather.Tests;

public sealed class NoaaSolarIndicesParsingTests
{
    [Fact]
    public void ParseSolarIndices_ValidText_ReturnsLastDataRow()
    {
        const string text = """
            # header line
            : another header
            2026 04 11   93     42      180      0    -999      *   2  0  0  1  0  0  0
            2026 04 12   99     47      270      1    -999      *   6  0  0  0  0  0  0
            """;

        var entry = NoaaDataParsers.ParseSolarIndicesText(text);

        Assert.Equal(99.0, entry.SolarFlux, precision: 1);
        Assert.Equal(47u, entry.SunspotNumber);
    }

    [Fact]
    public void ParseSolarIndices_AllComments_ThrowsParseError()
    {
        const string text = """
            # this is a comment
            : this is also a comment
            """;

        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseSolarIndicesText(text));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
        Assert.Contains("no data lines", ex.Message);
    }

    [Fact]
    public void ParseSolarIndices_EmptyInput_ThrowsParseError()
    {
        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseSolarIndicesText(""));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
    }

    [Fact]
    public void ParseSolarIndices_MalformedRow_ThrowsParseError()
    {
        const string text = "2026 04 12";

        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseSolarIndicesText(text));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
        Assert.Contains("missing expected columns", ex.Message);
    }

    [Fact]
    public void ParseSolarIndices_InvalidSolarFlux_ThrowsParseError()
    {
        const string text = "2026 04 12   abc   47";

        var ex = Assert.Throws<SpaceWeatherProviderException>(() =>
            NoaaDataParsers.ParseSolarIndicesText(text));

        Assert.Equal(SpaceWeatherProviderErrorKind.Parse, ex.Kind);
        Assert.Contains("solar flux", ex.Message);
    }
}
