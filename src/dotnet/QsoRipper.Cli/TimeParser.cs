using Google.Protobuf.WellKnownTypes;

namespace QsoRipper.Cli;

internal static class TimeParser
{
    public static Timestamp? Parse(string input)
    {
        if (TryParseRelative(input, out var dt))
        {
            return Timestamp.FromDateTime(dt);
        }

        if (DateTime.TryParse(input, null, System.Globalization.DateTimeStyles.AdjustToUniversal, out var parsed))
        {
            return Timestamp.FromDateTime(DateTime.SpecifyKind(parsed, DateTimeKind.Utc));
        }

        return null;
    }

    private static bool TryParseRelative(string input, out DateTime result)
    {
        result = default;
        var parts = input.Split('.');

        if (parts.Length != 2 || !int.TryParse(parts[0], out var count) || count < 0)
        {
            return false;
        }

        var unit = parts[1].ToLowerInvariant();
        var now = DateTime.UtcNow;

        result = unit switch
        {
            "minutes" or "minute" or "min" => now.AddMinutes(-count),
            "hours" or "hour" or "hr" => now.AddHours(-count),
            "days" or "day" => now.AddDays(-count),
            "weeks" or "week" => now.AddDays(-count * 7),
            "months" or "month" => now.AddMonths(-count),
            "years" or "year" => now.AddYears(-count),
            _ => default,
        };

        return result != default;
    }
}
