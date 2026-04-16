using QsoRipper.Domain;

namespace QsoRipper.Engine.SpaceWeather;

/// <summary>
/// Abstraction over an external current space weather data provider.
/// </summary>
public interface ISpaceWeatherProvider
{
    /// <summary>
    /// Fetch a fresh normalized current space weather snapshot.
    /// </summary>
    /// <exception cref="SpaceWeatherProviderException">When the provider cannot return data.</exception>
    SpaceWeatherSnapshot FetchCurrent();
}
