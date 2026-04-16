namespace QsoRipper.Engine.SpaceWeather;

/// <summary>
/// Stable error categories for the space weather provider layer.
/// </summary>
public enum SpaceWeatherProviderErrorKind
{
    /// <summary>Provider is disabled by configuration.</summary>
    Disabled,

    /// <summary>Transport failed before a valid response was received.</summary>
    Transport,

    /// <summary>Provider returned a payload that could not be parsed.</summary>
    Parse,
}

/// <summary>
/// Exception surfaced by the space weather provider layer.
/// </summary>
#pragma warning disable CA1032, RCS1194 // Standard constructors intentionally omitted — use factory methods.
public sealed class SpaceWeatherProviderException : Exception
#pragma warning restore CA1032, RCS1194
{
    /// <summary>Gets the stable error category.</summary>
    public SpaceWeatherProviderErrorKind Kind { get; }

    /// <summary>Gets whether the error class is suitable for retry handling.</summary>
    public bool IsRetryable => Kind == SpaceWeatherProviderErrorKind.Transport;

    private SpaceWeatherProviderException(SpaceWeatherProviderErrorKind kind, string message)
        : base(message)
    {
        Kind = kind;
    }

    /// <summary>Create an exception indicating the provider is disabled.</summary>
    public static SpaceWeatherProviderException Disabled(string message) =>
        new(SpaceWeatherProviderErrorKind.Disabled, message);

    /// <summary>Create an exception indicating a transport failure.</summary>
    public static SpaceWeatherProviderException Transport(string message) =>
        new(SpaceWeatherProviderErrorKind.Transport, message);

    /// <summary>Create an exception indicating a parse failure.</summary>
    public static SpaceWeatherProviderException Parse(string message) =>
        new(SpaceWeatherProviderErrorKind.Parse, message);
}
