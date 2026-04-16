#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods

namespace QsoRipper.Engine.SpaceWeather.Tests;

public sealed class GeomagneticStormScaleTests
{
    [Theory]
    [InlineData(9.0, 5u)]
    [InlineData(9.5, 5u)]
    [InlineData(8.0, 4u)]
    [InlineData(8.99, 4u)]
    [InlineData(7.0, 3u)]
    [InlineData(7.5, 3u)]
    [InlineData(6.0, 2u)]
    [InlineData(6.99, 2u)]
    [InlineData(5.0, 1u)]
    [InlineData(5.5, 1u)]
    [InlineData(4.99, 0u)]
    [InlineData(0.0, 0u)]
    [InlineData(2.33, 0u)]
    public void CalculateGeomagneticStormScale_ReturnsExpectedScale(double kp, uint expectedScale)
    {
        Assert.Equal(expectedScale, NoaaDataParsers.CalculateGeomagneticStormScale(kp));
    }
}
