using LogRipper.Domain;
using System.Reflection;
using Google.Protobuf.Reflection;

namespace LogRipper.DebugHost.Utilities;

public static class ProtoEnumDisplay
{
    public static string ForBand(Band band) => FormatEnumValue(band, "BAND");

    public static string ForMode(Mode mode) => FormatEnumValue(mode, "MODE");

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
}
