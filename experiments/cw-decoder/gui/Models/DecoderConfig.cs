using System.Globalization;

namespace CwDecoderGui.Models;

/// <summary>
/// Operator-tunable decoder configuration. Mirrors the Rust
/// <c>streaming::DecoderConfig</c> wire format. All values are in natural
/// units (dB / dimensionless scale) so the slider labels match the
/// underlying decoder semantics.
/// </summary>
public readonly record struct DecoderConfig(
    double MinSnrDb,
    double PitchMinSnrDb,
    double ThresholdScale)
{
    public const double DefaultMinSnrDb = 6.0;
    public const double DefaultPitchMinSnrDb = 8.0;
    public const double DefaultThresholdScale = 1.0;

    public static DecoderConfig Defaults => new(DefaultMinSnrDb, DefaultPitchMinSnrDb, DefaultThresholdScale);

    /// <summary>
    /// Render as CLI arguments for spawning the decoder with these initial
    /// values. The format is invariant-culture to avoid locale decimal
    /// separator surprises (e.g. "," in de-DE).
    /// </summary>
    public string ToCliArgs()
    {
        var ic = CultureInfo.InvariantCulture;
        return $"--min-snr-db {MinSnrDb.ToString(ic)} --pitch-min-snr-db {PitchMinSnrDb.ToString(ic)} --threshold-scale {ThresholdScale.ToString(ic)}";
    }

    /// <summary>NDJSON command for live config update over stdin.</summary>
    public string ToJsonCommand()
    {
        var ic = CultureInfo.InvariantCulture;
        return "{\"type\":\"config\",\"min_snr_db\":"
             + MinSnrDb.ToString(ic)
             + ",\"pitch_min_snr_db\":"
             + PitchMinSnrDb.ToString(ic)
             + ",\"threshold_scale\":"
             + ThresholdScale.ToString(ic)
             + "}";
    }
}
