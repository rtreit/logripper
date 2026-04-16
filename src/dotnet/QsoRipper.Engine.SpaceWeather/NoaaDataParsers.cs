using System.Globalization;
using System.Text.Json;
using Google.Protobuf.WellKnownTypes;

namespace QsoRipper.Engine.SpaceWeather;

/// <summary>Data parsed from a single NOAA planetary K-index JSON entry.</summary>
internal readonly record struct KpIndexEntry(string TimeTag, double Kp, uint ARunning);

/// <summary>Data parsed from the last NOAA daily solar indices row.</summary>
internal readonly record struct SolarIndicesEntry(double SolarFlux, uint SunspotNumber);

/// <summary>
/// Pure parsing functions for NOAA SWPC data feeds.
/// </summary>
internal static class NoaaDataParsers
{
    /// <summary>
    /// Parse the NOAA planetary K-index JSON and return the last entry.
    /// </summary>
    /// <remarks>
    /// The NOAA API returns a JSON array of objects, each with:
    /// <c>time_tag</c> (string), <c>Kp</c> (number), <c>a_running</c> (number), <c>station_count</c> (number).
    /// </remarks>
    internal static KpIndexEntry ParseKpIndexJson(string json)
    {
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(json);
        }
        catch (JsonException ex)
        {
            throw SpaceWeatherProviderException.Parse(
                $"Failed to parse NOAA planetary K-index JSON: {ex.Message}");
        }

        using (doc)
        {
            var root = doc.RootElement;

            if (root.ValueKind != JsonValueKind.Array || root.GetArrayLength() == 0)
            {
                throw SpaceWeatherProviderException.Parse(
                    "NOAA planetary K-index feed returned no entries.");
            }

            var last = root[root.GetArrayLength() - 1];

            try
            {
                var timeTag = last.GetProperty("time_tag").GetString()
                    ?? throw SpaceWeatherProviderException.Parse(
                        "NOAA planetary K-index entry has null time_tag.");

                var kp = last.GetProperty("Kp").GetDouble();
                var aRunning = last.GetProperty("a_running").GetUInt32();

                return new KpIndexEntry(timeTag, kp, aRunning);
            }
            catch (SpaceWeatherProviderException)
            {
                throw;
            }
            catch (KeyNotFoundException ex)
            {
                throw SpaceWeatherProviderException.Parse(
                    $"NOAA planetary K-index entry is missing required fields: {ex.Message}");
            }
            catch (Exception ex) when (ex is InvalidOperationException or FormatException or OverflowException)
            {
                throw SpaceWeatherProviderException.Parse(
                    $"NOAA planetary K-index entry has invalid field types: {ex.Message}");
            }
        }
    }

    /// <summary>
    /// Parse the NOAA daily solar indices text and return data from the last numeric row.
    /// </summary>
    /// <remarks>
    /// Header/comment lines start with <c>#</c> or <c>:</c>.
    /// Data rows are space-delimited: year month day solar_flux sunspot_number ...
    /// </remarks>
    internal static SolarIndicesEntry ParseSolarIndicesText(string text)
    {
        var line = text
            .Split('\n')
            .Select(static l => l.Trim())
            .Where(static l => l.Length > 0 && char.IsAsciiDigit(l[0]))
            .LastOrDefault()
            ?? throw SpaceWeatherProviderException.Parse(
                "NOAA daily solar indices feed returned no data lines.");

        var columns = line.Split(' ', StringSplitOptions.RemoveEmptyEntries);

        if (columns.Length < 5)
        {
            throw SpaceWeatherProviderException.Parse(
                $"NOAA daily solar indices line was missing expected columns: '{line}'");
        }

        if (!double.TryParse(columns[3], NumberStyles.Float, CultureInfo.InvariantCulture, out var solarFlux))
        {
            throw SpaceWeatherProviderException.Parse(
                $"Failed to parse NOAA solar flux '{columns[3]}'.");
        }

        if (!uint.TryParse(columns[4], NumberStyles.Integer, CultureInfo.InvariantCulture, out var sunspotNumber))
        {
            throw SpaceWeatherProviderException.Parse(
                $"Failed to parse NOAA sunspot number '{columns[4]}'.");
        }

        return new SolarIndicesEntry(solarFlux, sunspotNumber);
    }

    /// <summary>
    /// Calculate geomagnetic storm scale (G-scale) from planetary K-index.
    /// </summary>
    internal static uint CalculateGeomagneticStormScale(double kp)
    {
        return kp switch
        {
            >= 9.0 => 5,
            >= 8.0 => 4,
            >= 7.0 => 3,
            >= 6.0 => 2,
            >= 5.0 => 1,
            _ => 0,
        };
    }

    /// <summary>
    /// Parse a NOAA K-index timestamp (e.g. "2026-04-13T18:00:00") into a protobuf Timestamp.
    /// </summary>
    internal static Timestamp ParseTimestamp(string timeTag)
    {
        if (!DateTimeOffset.TryParseExact(
                timeTag,
                "yyyy-MM-dd'T'HH:mm:ss",
                CultureInfo.InvariantCulture,
                DateTimeStyles.AssumeUniversal | DateTimeStyles.AdjustToUniversal,
                out var parsed))
        {
            throw SpaceWeatherProviderException.Parse(
                $"Failed to parse NOAA K-index timestamp '{timeTag}'.");
        }

        return Timestamp.FromDateTimeOffset(parsed);
    }
}
