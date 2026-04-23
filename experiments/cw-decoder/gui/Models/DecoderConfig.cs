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
    double ThresholdScale,
    bool AutoThreshold,
    bool ExperimentalRangeLock,
    double RangeLockMinHz,
    double RangeLockMaxHz,
    double MinTonePurity,
    double ForcePitchHz,
    int WideBinCount,
    double MinPulseDotFraction,
    double MinGapDotFraction)
{
    public const double DefaultMinSnrDb = 3.0;
    public const double DefaultPitchMinSnrDb = 6.0;
    public const double DefaultThresholdScale = 1.0;
    public const bool DefaultAutoThreshold = true;
    public const bool DefaultExperimentalRangeLock = false;
    public const double DefaultRangeLockMinHz = 550.0;
    public const double DefaultRangeLockMaxHz = 850.0;
    /// <summary>
    /// Default minimum instantaneous adjacent-bin tone-purity ratio
    /// (target / max(adjacent purity bin)). Mirrors Rust
    /// <c>streaming::DEFAULT_MIN_TONE_PURITY</c>. Set to 0 to disable.
    /// </summary>
    public const double DefaultMinTonePurity = 3.0;
    /// <summary>
    /// 0 = auto pitch acquisition. Anything &gt; 0 forces the decoder
    /// to lock to that exact pitch and disables the Fisher watchdog.
    /// Useful for live capture where the operator knows the target tone.
    /// </summary>
    public const double DefaultForcePitchHz = 0.0;
    /// <summary>
    /// Number of side bins per side added to the target Goertzel.
    /// 0 = single 40-Hz-wide integration. 2 ≈ 200 Hz of bandwidth,
    /// useful for acoustically re-captured CW where the signal is
    /// smeared across many bins.
    /// </summary>
    public const int DefaultWideBinCount = 0;
    /// <summary>
    /// Drop on-runs shorter than this fraction of one estimated dot
    /// length. 0 = disabled. 0.3 is a good mic-mode value to suppress
    /// constant-noise ghost characters in silent stretches without
    /// killing real fast-keyed dits.
    /// </summary>
    public const double DefaultMinPulseDotFraction = 0.0;
    /// <summary>
    /// Bridge off-runs shorter than this fraction of one estimated dot
    /// length. 0 = disabled. Twin of <see cref="DefaultMinPulseDotFraction"/>.
    /// 0.3 stops a real dah from being fragmented into adjacent dits
    /// when the mic envelope chatters around threshold inside a key-down.
    /// </summary>
    public const double DefaultMinGapDotFraction = 0.0;

    public static DecoderConfig Defaults => new(
        DefaultMinSnrDb,
        DefaultPitchMinSnrDb,
        DefaultThresholdScale,
        DefaultAutoThreshold,
        DefaultExperimentalRangeLock,
        DefaultRangeLockMinHz,
        DefaultRangeLockMaxHz,
        DefaultMinTonePurity,
        DefaultForcePitchHz,
        DefaultWideBinCount,
        DefaultMinPulseDotFraction,
        DefaultMinGapDotFraction);

    /// <summary>
    /// Render as CLI arguments for spawning the decoder with these initial
    /// values. The format is invariant-culture to avoid locale decimal
    /// separator surprises (e.g. "," in de-DE).
    /// </summary>
    public string ToCliArgs()
    {
        var ic = CultureInfo.InvariantCulture;
        var args = $"--min-snr-db {MinSnrDb.ToString(ic)} --pitch-min-snr-db {PitchMinSnrDb.ToString(ic)} --threshold-scale {ThresholdScale.ToString(ic)}";
        if (!AutoThreshold)
        {
            args += " --no-auto-threshold";
        }
        if (ExperimentalRangeLock)
        {
            args += $" --experimental-range-lock --range-lock-min-hz {RangeLockMinHz.ToString(ic)} --range-lock-max-hz {RangeLockMaxHz.ToString(ic)}";
        }
        args += $" --min-tone-purity {MinTonePurity.ToString(ic)}";
        if (ForcePitchHz > 0.0)
        {
            args += $" --force-pitch-hz {ForcePitchHz.ToString(ic)}";
        }
        if (WideBinCount > 0)
        {
            args += $" --wide-bin-count {WideBinCount.ToString(ic)}";
        }
        if (MinPulseDotFraction > 0.0)
        {
            args += $" --min-pulse-dot-fraction {MinPulseDotFraction.ToString(ic)}";
        }
        if (MinGapDotFraction > 0.0)
        {
            args += $" --min-gap-dot-fraction {MinGapDotFraction.ToString(ic)}";
        }
        return args;
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
             + ",\"auto_threshold\":"
             + (AutoThreshold ? "true" : "false")
             + ",\"experimental_range_lock\":"
             + (ExperimentalRangeLock ? "true" : "false")
             + ",\"range_lock_min_hz\":"
             + RangeLockMinHz.ToString(ic)
             + ",\"range_lock_max_hz\":"
             + RangeLockMaxHz.ToString(ic)
             + ",\"min_tone_purity\":"
             + MinTonePurity.ToString(ic)
             + ",\"force_pitch_hz\":"
             + (ForcePitchHz > 0.0 ? ForcePitchHz.ToString(ic) : "null")
             + ",\"wide_bin_count\":"
             + WideBinCount.ToString(ic)
             + ",\"min_pulse_dot_fraction\":"
             + MinPulseDotFraction.ToString(ic)
             + ",\"min_gap_dot_fraction\":"
             + MinGapDotFraction.ToString(ic)
             + "}";
    }
}
