using System.Globalization;

namespace CwDecoderGui.Models;

internal readonly record struct BaselineDecoderConfig(
    double WindowSeconds,
    double MinWindowSeconds,
    int DecodeEveryMs,
    int Confirmations)
{
    public string ToCliArgs()
    {
        var ic = CultureInfo.InvariantCulture;
        return $"--window {WindowSeconds.ToString(ic)} --min-window {MinWindowSeconds.ToString(ic)} --decode-every-ms {DecodeEveryMs.ToString(ic)} --confirmations {Confirmations.ToString(ic)}";
    }
}
