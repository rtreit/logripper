using Google.Protobuf.WellKnownTypes;
using QsoRipper.Cli.Commands;
using QsoRipper.Domain;

namespace QsoRipper.Cli.Tests;

#pragma warning disable CA1707
[Collection("ConsoleCapture")]
public sealed class SpaceWeatherCommandTests
{
    [Fact]
    public void HandleSnapshot_returns_error_when_snapshot_is_missing()
    {
        var error = ConsoleCapture.Error(() => Assert.Equal(1, SpaceWeatherCommand.HandleSnapshot(null, false)));

        Assert.Contains("Space weather snapshot unavailable.", error, StringComparison.Ordinal);
    }

    [Fact]
    public void HandleSnapshot_writes_text_output_for_current_snapshot()
    {
        var snapshot = new SpaceWeatherSnapshot
        {
            Status = SpaceWeatherStatus.Current,
            ObservedAt = Timestamp.FromDateTime(DateTime.SpecifyKind(new DateTime(2026, 4, 14, 1, 2, 3), DateTimeKind.Utc)),
            FetchedAt = Timestamp.FromDateTime(DateTime.SpecifyKind(new DateTime(2026, 4, 14, 1, 5, 0), DateTimeKind.Utc)),
            PlanetaryKIndex = 3.67,
            PlanetaryAIndex = 12,
            SolarFluxIndex = 145.2,
            SunspotNumber = 88,
            GeomagneticStormScale = 1,
            SourceName = "NOAA SWPC"
        };

        var output = ConsoleCapture.Out(() => Assert.Equal(0, SpaceWeatherCommand.HandleSnapshot(snapshot, false)));

        Assert.Contains("Status:           current", output, StringComparison.Ordinal);
        Assert.Contains("Planetary K:      3.67", output, StringComparison.Ordinal);
        Assert.Contains("Planetary A:      12", output, StringComparison.Ordinal);
        Assert.Contains("Solar flux:       145.2", output, StringComparison.Ordinal);
        Assert.Contains("Sunspot number:   88", output, StringComparison.Ordinal);
        Assert.Contains("Storm scale:      G1", output, StringComparison.Ordinal);
        Assert.Contains("Source:           NOAA SWPC", output, StringComparison.Ordinal);
    }

    [Fact]
    public void HandleSnapshot_returns_error_for_error_snapshot()
    {
        var snapshot = new SpaceWeatherSnapshot
        {
            Status = SpaceWeatherStatus.Error,
            ErrorMessage = "NOAA unavailable"
        };

        var output = ConsoleCapture.Out(() => Assert.Equal(1, SpaceWeatherCommand.HandleSnapshot(snapshot, false)));

        Assert.Contains("Status:           error", output, StringComparison.Ordinal);
        Assert.Contains("Error:            NOAA unavailable", output, StringComparison.Ordinal);
    }

    [Fact]
    public void HandleSnapshot_writes_json_when_requested()
    {
        var snapshot = new SpaceWeatherSnapshot
        {
            Status = SpaceWeatherStatus.Stale,
            SourceName = "NOAA SWPC"
        };

        var output = ConsoleCapture.Out(() => Assert.Equal(0, SpaceWeatherCommand.HandleSnapshot(snapshot, true)));

        Assert.Contains("\"status\": \"SPACE_WEATHER_STATUS_STALE\"", output, StringComparison.Ordinal);
        Assert.Contains("\"sourceName\": \"NOAA SWPC\"", output, StringComparison.Ordinal);
    }
}
#pragma warning restore CA1707

