using System.Collections.Generic;
using System.Linq;
using System.Reflection;
using Google.Protobuf.Reflection;
using QsoRipper.Domain;

namespace QsoRipper.Gui.Utilities;

internal static class ProtoEnumDisplay
{
    public static string ForBand(Band band) =>
        band == Band.Unspecified ? "-" : FormatEnumValue(band, "BAND");

    public static string ForMode(Mode mode) =>
        mode == Mode.Unspecified ? "-" : FormatEnumValue(mode, "MODE");

    public static bool TryParseBand(string? value, out Band band) =>
        TryParseEnum(value, Band.Unspecified, ForBand, out band);

    public static bool TryParseMode(string? value, out Mode mode) =>
        TryParseEnum(value, Mode.Unspecified, ForMode, out mode);

    private static string FormatEnumValue<TEnum>(TEnum value, string prefix)
        where TEnum : struct, Enum
    {
        var field = typeof(TEnum).GetField(value.ToString());
        var originalName = field?.GetCustomAttribute<OriginalNameAttribute>()?.Name ?? value.ToString().ToUpperInvariant();

        if (originalName.StartsWith(prefix + "_", StringComparison.Ordinal))
        {
            originalName = originalName[(prefix.Length + 1)..];
        }

        return originalName.Replace('_', '.');
    }

    private static bool TryParseEnum<TEnum>(
        string? value,
        TEnum unspecified,
        Func<TEnum, string> formatter,
        out TEnum parsedValue)
        where TEnum : struct, Enum
    {
        var normalizedInput = NormalizeToken(value);
        if (normalizedInput.Length == 0)
        {
            parsedValue = unspecified;
            return false;
        }

        foreach (var candidate in Enum.GetValues<TEnum>())
        {
            if (EqualityComparer<TEnum>.Default.Equals(candidate, unspecified))
            {
                continue;
            }

            if (NormalizeToken(formatter(candidate)) == normalizedInput)
            {
                parsedValue = candidate;
                return true;
            }
        }

        parsedValue = unspecified;
        return false;
    }

    private static string NormalizeToken(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return string.Empty;
        }

        return string.Concat(value.Where(char.IsLetterOrDigit)).ToUpperInvariant();
    }
}
