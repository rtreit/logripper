using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;

namespace QsoRipper.Engine.SpaceWeather;

/// <summary>
/// On-demand cache and refresh layer for space weather data.
/// Thread-safe; no background polling.
/// </summary>
public sealed class SpaceWeatherMonitor
{
    private readonly ISpaceWeatherProvider _provider;
    private readonly TimeSpan _refreshInterval;
    private readonly TimeSpan _staleAfter;
    private readonly TimeProvider _clock;
    private readonly Lock _lock = new();
    private SpaceWeatherSnapshot? _cached;
    private DateTimeOffset _lastFetchTime;

    public SpaceWeatherMonitor(
        ISpaceWeatherProvider provider,
        TimeSpan refreshInterval,
        TimeSpan staleAfter,
        TimeProvider? clock = null)
    {
        ArgumentNullException.ThrowIfNull(provider);

        _provider = provider;
        _refreshInterval = refreshInterval;
        _staleAfter = staleAfter;
        _clock = clock ?? TimeProvider.System;
    }

    /// <summary>
    /// Return the cached snapshot if still fresh, otherwise fetch a new one.
    /// </summary>
    public SpaceWeatherSnapshot CurrentSnapshot()
    {
        lock (_lock)
        {
            if (_cached is not null)
            {
                var elapsed = _clock.GetUtcNow() - _lastFetchTime;

                if (elapsed < _refreshInterval)
                {
                    if (elapsed >= _staleAfter)
                    {
                        var stale = _cached.Clone();
                        stale.Status = SpaceWeatherStatus.Stale;
                        return stale;
                    }

                    return _cached.Clone();
                }
            }

            return RefreshNoLock();
        }
    }

    /// <summary>
    /// Force a fresh fetch from the provider, ignoring any cached data.
    /// </summary>
    public SpaceWeatherSnapshot RefreshSnapshot()
    {
        lock (_lock)
        {
            return RefreshNoLock();
        }
    }

    private SpaceWeatherSnapshot RefreshNoLock()
    {
        try
        {
            var snapshot = _provider.FetchCurrent();
            var now = _clock.GetUtcNow();
            snapshot.FetchedAt = Timestamp.FromDateTimeOffset(now);
            _cached = snapshot;
            _lastFetchTime = now;
            return snapshot.Clone();
        }
        catch (SpaceWeatherProviderException ex)
        {
            if (_cached is not null)
            {
                var stale = _cached.Clone();
                stale.Status = SpaceWeatherStatus.Stale;
                stale.ErrorMessage = ex.Message;
                return stale;
            }

            return new SpaceWeatherSnapshot
            {
                Status = SpaceWeatherStatus.Error,
                ErrorMessage = ex.Message,
                SourceName = "NOAA SWPC",
            };
        }
    }
}
