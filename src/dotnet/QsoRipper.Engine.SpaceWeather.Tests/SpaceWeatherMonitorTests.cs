#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
#pragma warning disable CA1307 // Use StringComparison for string comparison

using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;

namespace QsoRipper.Engine.SpaceWeather.Tests;

public sealed class SpaceWeatherMonitorTests
{
    private static readonly TimeSpan RefreshInterval = TimeSpan.FromMinutes(15);
    private static readonly TimeSpan StaleAfter = TimeSpan.FromHours(1);

    [Fact]
    public void CurrentSnapshot_NoCacheFetchesFromProvider()
    {
        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        var result = monitor.CurrentSnapshot();

        Assert.Equal(SpaceWeatherStatus.Current, result.Status);
        Assert.Equal("NOAA SWPC", result.SourceName);
        Assert.Equal(1, provider.FetchCount);
    }

    [Fact]
    public void CurrentSnapshot_FreshCacheReturnsWithoutFetching()
    {
        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        monitor.CurrentSnapshot(); // populate cache
        clock.Advance(TimeSpan.FromMinutes(5)); // still fresh
        var result = monitor.CurrentSnapshot();

        Assert.Equal(SpaceWeatherStatus.Current, result.Status);
        Assert.Equal(1, provider.FetchCount); // no re-fetch
    }

    [Fact]
    public void CurrentSnapshot_ExpiredCacheRefreshes()
    {
        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        monitor.CurrentSnapshot();
        clock.Advance(TimeSpan.FromMinutes(20)); // past refresh interval
        monitor.CurrentSnapshot();

        Assert.Equal(2, provider.FetchCount);
    }

    [Fact]
    public void RefreshSnapshot_AlwaysFetches()
    {
        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        monitor.CurrentSnapshot(); // populate cache
        monitor.RefreshSnapshot(); // force refresh even when cache is fresh

        Assert.Equal(2, provider.FetchCount);
    }

    [Fact]
    public void FetchFailure_WithCache_ReturnsStaleCached()
    {
        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        monitor.CurrentSnapshot(); // populate cache
        provider.FailWith(SpaceWeatherProviderException.Transport("connection refused"));
        clock.Advance(TimeSpan.FromMinutes(20)); // expire cache

        var result = monitor.CurrentSnapshot();

        Assert.Equal(SpaceWeatherStatus.Stale, result.Status);
        Assert.Contains("connection refused", result.ErrorMessage);
        Assert.Equal(2.33, result.PlanetaryKIndex, precision: 2); // original data preserved
    }

    [Fact]
    public void FetchFailure_NoCache_ReturnsErrorSnapshot()
    {
        var provider = new FakeProvider(null);
        provider.FailWith(SpaceWeatherProviderException.Transport("connection refused"));
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        var result = monitor.CurrentSnapshot();

        Assert.Equal(SpaceWeatherStatus.Error, result.Status);
        Assert.Contains("connection refused", result.ErrorMessage);
    }

    [Fact]
    public void CurrentSnapshot_StaleCacheBeforeRefreshInterval_MarksStale()
    {
        // Use unusual config where stale_after < refresh_interval
        var shortStaleAfter = TimeSpan.FromMinutes(5);
        var longRefreshInterval = TimeSpan.FromMinutes(30);

        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, longRefreshInterval, shortStaleAfter, clock);

        monitor.CurrentSnapshot();
        clock.Advance(TimeSpan.FromMinutes(10)); // past stale but before refresh

        var result = monitor.CurrentSnapshot();

        Assert.Equal(SpaceWeatherStatus.Stale, result.Status);
        Assert.Equal(1, provider.FetchCount); // no re-fetch
    }

    [Fact]
    public void RefreshSnapshot_Failure_WithCache_ReturnsStaleCached()
    {
        var provider = new FakeProvider(MakeSampleSnapshot());
        var clock = new TestClock(DateTimeOffset.UtcNow);
        var monitor = new SpaceWeatherMonitor(provider, RefreshInterval, StaleAfter, clock);

        monitor.CurrentSnapshot(); // populate cache
        provider.FailWith(SpaceWeatherProviderException.Parse("bad data"));

        var result = monitor.RefreshSnapshot();

        Assert.Equal(SpaceWeatherStatus.Stale, result.Status);
        Assert.Contains("bad data", result.ErrorMessage);
    }

    private static SpaceWeatherSnapshot MakeSampleSnapshot() => new()
    {
        ObservedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
        Status = SpaceWeatherStatus.Current,
        PlanetaryKIndex = 2.33,
        PlanetaryAIndex = 9,
        SolarFluxIndex = 148.0,
        SunspotNumber = 96,
        GeomagneticStormScale = 0,
        SourceName = "NOAA SWPC",
    };

    private sealed class FakeProvider : ISpaceWeatherProvider
    {
        private SpaceWeatherSnapshot? _response;
        private SpaceWeatherProviderException? _error;

        public int FetchCount { get; private set; }

        public FakeProvider(SpaceWeatherSnapshot? response)
        {
            _response = response;
        }

        public void FailWith(SpaceWeatherProviderException error)
        {
            _error = error;
            _response = null;
        }

        public SpaceWeatherSnapshot FetchCurrent()
        {
            FetchCount++;

            if (_error is not null)
            {
                throw _error;
            }

            return _response?.Clone()
                ?? throw new InvalidOperationException("No response configured.");
        }
    }

    private sealed class TestClock(DateTimeOffset start) : TimeProvider
    {
        private DateTimeOffset _now = start;

        public void Advance(TimeSpan duration) => _now += duration;

        public override DateTimeOffset GetUtcNow() => _now;
    }
}
