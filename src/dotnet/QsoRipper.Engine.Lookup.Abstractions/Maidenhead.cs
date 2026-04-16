namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Maidenhead grid-square to coordinate conversion.
/// </summary>
internal static class Maidenhead
{
    /// <summary>
    /// Convert a Maidenhead grid square locator to (latitude, longitude) center coordinates.
    /// Supports 4, 6, and 8 character locators.
    /// </summary>
    public static (double Latitude, double Longitude)? GridToLatLon(string? grid)
    {
        if (string.IsNullOrWhiteSpace(grid))
        {
            return null;
        }

        var trimmed = grid.Trim();
        var len = trimmed.Length;
        if (len < 4 || len % 2 != 0 || len > 8)
        {
            return null;
        }

        // Field (first pair): A-R
        var fieldLon = LetterValue(trimmed[0], 'A', 'R');
        var fieldLat = LetterValue(trimmed[1], 'A', 'R');
        if (fieldLon < 0 || fieldLat < 0)
        {
            return null;
        }

        // Square (second pair): 0-9
        var squareLon = DigitValue(trimmed[2]);
        var squareLat = DigitValue(trimmed[3]);
        if (squareLon < 0 || squareLat < 0)
        {
            return null;
        }

        var longitude = fieldLon * 20.0 - 180.0 + squareLon * 2.0;
        var latitude = fieldLat * 10.0 - 90.0 + squareLat;

        var lonStep = 2.0;
        var latStep = 1.0;

        if (len >= 6)
        {
            // Subsquare (third pair): a-x (case insensitive)
            var subLon = LetterValue(trimmed[4], 'A', 'X');
            var subLat = LetterValue(trimmed[5], 'A', 'X');
            if (subLon < 0 || subLat < 0)
            {
                return null;
            }

            lonStep = 2.0 / 24.0;
            latStep = 1.0 / 24.0;
            longitude += subLon * lonStep;
            latitude += subLat * latStep;
        }

        if (len >= 8)
        {
            // Extended square (fourth pair): 0-9
            var extLon = DigitValue(trimmed[6]);
            var extLat = DigitValue(trimmed[7]);
            if (extLon < 0 || extLat < 0)
            {
                return null;
            }

            var extLonStep = lonStep / 10.0;
            var extLatStep = latStep / 10.0;
            longitude += extLon * extLonStep;
            latitude += extLat * extLatStep;
            lonStep = extLonStep;
            latStep = extLatStep;
        }

        // Return center of the grid cell
        longitude += lonStep / 2.0;
        latitude += latStep / 2.0;

        return (latitude, longitude);
    }

    private static int LetterValue(char ch, char min, char max)
    {
        var upper = char.ToUpperInvariant(ch);
        if (upper >= min && upper <= max)
        {
            return upper - min;
        }

        return -1;
    }

    private static int DigitValue(char ch)
    {
        if (ch is >= '0' and <= '9')
        {
            return ch - '0';
        }

        return -1;
    }
}
