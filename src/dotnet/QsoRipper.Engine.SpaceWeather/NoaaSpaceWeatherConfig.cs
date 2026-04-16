namespace QsoRipper.Engine.SpaceWeather;

/// <summary>
/// Configuration for the NOAA SWPC space weather provider.
/// </summary>
public sealed class NoaaSpaceWeatherConfig
{
    /// <summary>Environment variable controlling whether NOAA fetching is enabled.</summary>
    public const string EnabledEnvVar = "QSORIPPER_NOAA_SPACE_WEATHER_ENABLED";

    /// <summary>Environment variable overriding the refresh interval in seconds.</summary>
    public const string RefreshIntervalEnvVar = "QSORIPPER_NOAA_REFRESH_INTERVAL_SECONDS";

    /// <summary>Environment variable overriding the stale-after threshold in seconds.</summary>
    public const string StaleAfterEnvVar = "QSORIPPER_NOAA_STALE_AFTER_SECONDS";

    /// <summary>Environment variable overriding the HTTP timeout in seconds.</summary>
    public const string TimeoutEnvVar = "QSORIPPER_NOAA_TIMEOUT_SECONDS";

    /// <summary>Default refresh interval (15 minutes).</summary>
    public const int DefaultRefreshIntervalSeconds = 900;

    /// <summary>Default stale-after threshold (1 hour).</summary>
    public const int DefaultStaleAfterSeconds = 3600;

    /// <summary>Default HTTP timeout.</summary>
    public const int DefaultTimeoutSeconds = 8;

    /// <summary>Default NOAA planetary K-index endpoint.</summary>
    public const string DefaultKpIndexUrl = "https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json";

    /// <summary>Default NOAA daily solar indices endpoint.</summary>
    public const string DefaultSolarIndicesUrl = "https://services.swpc.noaa.gov/text/daily-solar-indices.txt";

    /// <summary>Gets or sets whether NOAA space weather fetching is enabled.</summary>
    public bool Enabled { get; init; } = true;

    /// <summary>Gets or sets the K-index JSON endpoint URL.</summary>
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Design", "CA1056:URI-like properties should not be strings", Justification = "Stored as string from env vars; passed directly to HttpRequestMessage.")]
    public string KpIndexUrl { get; init; } = DefaultKpIndexUrl;

    /// <summary>Gets or sets the daily solar indices text endpoint URL.</summary>
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Design", "CA1056:URI-like properties should not be strings", Justification = "Stored as string from env vars; passed directly to HttpRequestMessage.")]
    public string SolarIndicesUrl { get; init; } = DefaultSolarIndicesUrl;

    /// <summary>Gets or sets the HTTP request timeout.</summary>
    public TimeSpan HttpTimeout { get; init; } = TimeSpan.FromSeconds(DefaultTimeoutSeconds);

    /// <summary>Gets or sets the snapshot refresh interval.</summary>
    public TimeSpan RefreshInterval { get; init; } = TimeSpan.FromSeconds(DefaultRefreshIntervalSeconds);

    /// <summary>Gets or sets the stale-after threshold.</summary>
    public TimeSpan StaleAfter { get; init; } = TimeSpan.FromSeconds(DefaultStaleAfterSeconds);

    /// <summary>
    /// Load provider configuration from environment variables.
    /// </summary>
    public static NoaaSpaceWeatherConfig FromEnvironment()
    {
        var enabled = ParseBool(
            Environment.GetEnvironmentVariable(EnabledEnvVar),
            defaultValue: true);
        var refreshInterval = ParseSeconds(
            Environment.GetEnvironmentVariable(RefreshIntervalEnvVar),
            DefaultRefreshIntervalSeconds);
        var staleAfter = ParseSeconds(
            Environment.GetEnvironmentVariable(StaleAfterEnvVar),
            DefaultStaleAfterSeconds);
        var timeout = ParseSeconds(
            Environment.GetEnvironmentVariable(TimeoutEnvVar),
            DefaultTimeoutSeconds);

        return new NoaaSpaceWeatherConfig
        {
            Enabled = enabled,
            RefreshInterval = TimeSpan.FromSeconds(refreshInterval),
            StaleAfter = TimeSpan.FromSeconds(staleAfter),
            HttpTimeout = TimeSpan.FromSeconds(timeout),
        };
    }

    private static bool ParseBool(string? raw, bool defaultValue)
    {
        if (string.IsNullOrWhiteSpace(raw))
        {
            return defaultValue;
        }

        return raw.Trim().ToUpperInvariant() switch
        {
            "1" or "TRUE" or "YES" or "Y" or "ON" => true,
            "0" or "FALSE" or "NO" or "N" or "OFF" => false,
            _ => defaultValue,
        };
    }

    private static int ParseSeconds(string? raw, int defaultValue)
    {
        if (string.IsNullOrWhiteSpace(raw))
        {
            return defaultValue;
        }

        return int.TryParse(raw.Trim(), System.Globalization.NumberStyles.Integer, System.Globalization.CultureInfo.InvariantCulture, out var value)
            ? value
            : defaultValue;
    }
}
