using QsoRipper.Domain;

namespace QsoRipper.Engine.SpaceWeather;

/// <summary>
/// Provider that always returns an error indicating space weather fetching is disabled.
/// </summary>
public sealed class DisabledSpaceWeatherProvider : ISpaceWeatherProvider
{
    private readonly string _reason;

    public DisabledSpaceWeatherProvider(string reason)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(reason);
        _reason = reason;
    }

    /// <inheritdoc/>
    public SpaceWeatherSnapshot FetchCurrent()
    {
        throw SpaceWeatherProviderException.Disabled(_reason);
    }
}
