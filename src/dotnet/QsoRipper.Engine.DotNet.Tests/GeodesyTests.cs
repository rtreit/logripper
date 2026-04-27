using Grpc.Core;
using QsoRipper.Domain;
using QsoRipper.Engine.DotNet;
using QsoRipper.Services;

namespace QsoRipper.Engine.DotNet.Tests;

#pragma warning disable CA1707 // xUnit allows underscores in test method names
public sealed class GeodesyTests
{
    [Fact]
    public void DistanceParisToNewYorkIsAbout5837Km()
    {
        var d = Geodesy.DistanceKm(
            new GeoPoint { Latitude = 48.8566, Longitude = 2.3522 },
            new GeoPoint { Latitude = 40.7128, Longitude = -74.0060 });
        Assert.InRange(d, 5817.0, 5857.0);
    }

    [Fact]
    public void DistanceSeattleToLondonIsAbout7720Km()
    {
        var d = Geodesy.DistanceKm(
            new GeoPoint { Latitude = 47.45, Longitude = -122.31 },
            new GeoPoint { Latitude = 51.47, Longitude = -0.46 });
        Assert.InRange(d, 7690.0, 7750.0);
    }

    [Fact]
    public void InitialBearingSeattleToNewYork()
    {
        var b = Geodesy.InitialBearingDeg(
            new GeoPoint { Latitude = 47.45, Longitude = -122.31 },
            new GeoPoint { Latitude = 40.7128, Longitude = -74.0060 });
        Assert.NotNull(b);
        Assert.InRange(b!.Value, 73.0, 83.0);
    }

    [Fact]
    public void BearingUndefinedForSamePoint()
    {
        var b = Geodesy.InitialBearingDeg(
            new GeoPoint { Latitude = 0.0, Longitude = 0.0 },
            new GeoPoint { Latitude = 0.0, Longitude = 0.0 });
        Assert.Null(b);
    }

    [Fact]
    public void BearingUndefinedForAntipodalPoints()
    {
        var b = Geodesy.InitialBearingDeg(
            new GeoPoint { Latitude = 10.0, Longitude = 20.0 },
            new GeoPoint { Latitude = -10.0, Longitude = -160.0 });
        Assert.Null(b);
    }

    [Fact]
    public void SamplesEndpointsMatchInputs()
    {
        var origin = new GeoPoint { Latitude = 47.45, Longitude = -122.31 };
        var target = new GeoPoint { Latitude = 40.7128, Longitude = -74.0060 };
        var samples = Geodesy.SampleGreatCircle(origin, target, 16);
        Assert.Equal(16, samples.Length);
        Assert.Equal(origin.Latitude, samples[0].Latitude, 6);
        Assert.Equal(origin.Longitude, samples[0].Longitude, 6);
        Assert.Equal(target.Latitude, samples[15].Latitude, 6);
        Assert.Equal(target.Longitude, samples[15].Longitude, 6);
    }

    [Fact]
    public void Maidenhead4CharCenter()
    {
        var p = Geodesy.MaidenheadToGeoPoint("CN87");
        Assert.Equal(-123.0, p.Longitude, 6);
        Assert.Equal(47.5, p.Latitude, 6);
    }

    [Fact]
    public void MaidenheadCaseInsensitive()
    {
        var upper = Geodesy.MaidenheadToGeoPoint("CN87WN");
        var lower = Geodesy.MaidenheadToGeoPoint("cn87wn");
        Assert.Equal(upper.Latitude, lower.Latitude, 9);
        Assert.Equal(upper.Longitude, lower.Longitude, 9);
    }

    [Fact]
    public void MaidenheadRejectsBadInput()
    {
        Assert.Throws<ArgumentException>(() => Geodesy.MaidenheadToGeoPoint(""));
        Assert.Throws<ArgumentException>(() => Geodesy.MaidenheadToGeoPoint("CN8"));
        Assert.Throws<ArgumentException>(() => Geodesy.MaidenheadToGeoPoint("ZZ87"));
    }

    [Fact]
    public void ResolveSampleCountDefaults()
    {
        Assert.Equal(Geodesy.DefaultSampleCount, Geodesy.ResolveSampleCount(0));
        Assert.Equal(2u, Geodesy.ResolveSampleCount(2));
        Assert.Equal(64u, Geodesy.ResolveSampleCount(64));
        Assert.Throws<ArgumentOutOfRangeException>(() => Geodesy.ResolveSampleCount(1));
        Assert.Throws<ArgumentOutOfRangeException>(() => Geodesy.ResolveSampleCount(1024));
    }

    [Fact]
    public void ValidatePointRejectsOutOfRange()
    {
        Assert.Throws<ArgumentException>(() =>
            Geodesy.ValidatePoint(new GeoPoint { Latitude = 91.0, Longitude = 0.0 }, "p"));
        Assert.Throws<ArgumentException>(() =>
            Geodesy.ValidatePoint(new GeoPoint { Latitude = 0.0, Longitude = 181.0 }, "p"));
        Assert.Throws<ArgumentException>(() =>
            Geodesy.ValidatePoint(new GeoPoint { Latitude = double.NaN, Longitude = 0.0 }, "p"));
    }
}
#pragma warning restore CA1707
