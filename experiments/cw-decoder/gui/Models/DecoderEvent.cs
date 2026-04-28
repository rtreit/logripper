using System.Text.Json.Serialization;

namespace CwDecoderGui.Models;

/// <summary>
/// Discriminated event from the Rust cw-decoder process. The "type" field
/// drives the polymorphic handling in <see cref="Services.CwDecoderProcess"/>.
/// We use a single bag-of-fields model rather than polymorphic deserialization
/// to keep the parse path allocation-free in the steady state.
/// </summary>
internal sealed class DecoderEvent
{
    [JsonPropertyName("type")] public string Type { get; set; } = "";
    [JsonPropertyName("t")] public double T { get; set; }

    // ready
    [JsonPropertyName("source")] public string? Source { get; set; }
    [JsonPropertyName("device")] public string? Device { get; set; }
    [JsonPropertyName("path")] public string? Path { get; set; }
    [JsonPropertyName("rate")] public int? Rate { get; set; }
    [JsonPropertyName("duration")] public double? Duration { get; set; }

    // pitch
    [JsonPropertyName("hz")] public double? Hz { get; set; }

    // wpm
    [JsonPropertyName("wpm")] public double? Wpm { get; set; }

    // char / garbled
    [JsonPropertyName("ch")] public string? Ch { get; set; }
    [JsonPropertyName("morse")] public string? Morse { get; set; }
    [JsonPropertyName("purity")] public double? Purity { get; set; }

    // power
    [JsonPropertyName("power")] public double? Power { get; set; }
    [JsonPropertyName("threshold")] public double? Threshold { get; set; }
    [JsonPropertyName("noise")] public double? Noise { get; set; }
    [JsonPropertyName("snr")] public double? Snr { get; set; }
    [JsonPropertyName("signal")] public bool? Signal { get; set; }

    // end
    [JsonPropertyName("transcript")] public string? Transcript { get; set; }
    [JsonPropertyName("pitch")] public double? Pitch { get; set; }

    // recording (ready / end)
    [JsonPropertyName("recording")] public string? Recording { get; set; }

    // viz (from stream-live-v3)
    [JsonPropertyName("envelope")] public double[]? Envelope { get; set; }
    [JsonPropertyName("envelope_max")] public double? EnvelopeMax { get; set; }
    [JsonPropertyName("noise_floor")] public double? NoiseFloor { get; set; }
    [JsonPropertyName("signal_floor")] public double? SignalFloor { get; set; }
    [JsonPropertyName("hyst_high")] public double? HystHigh { get; set; }
    [JsonPropertyName("hyst_low")] public double? HystLow { get; set; }
    [JsonPropertyName("buffer_seconds")] public double? BufferSeconds { get; set; }
    [JsonPropertyName("frame_step_s")] public double? FrameStepS { get; set; }
    [JsonPropertyName("dot_seconds")] public double? DotSeconds { get; set; }
    [JsonPropertyName("centroid_dot")] public double? CentroidDot { get; set; }
    [JsonPropertyName("centroid_dah")] public double? CentroidDah { get; set; }
    [JsonPropertyName("locked_wpm")] public double? LockedWpm { get; set; }
    [JsonPropertyName("on_durations")] public double[]? OnDurations { get; set; }
    [JsonPropertyName("events")] public VizEventDto[]? Events { get; set; }
    [JsonPropertyName("appended")] public string? Appended { get; set; }
    [JsonPropertyName("pitch_hz")] public double? PitchHz { get; set; }

    // decode-and-play extensions
    [JsonPropertyName("position")] public double? Position { get; set; }
    [JsonPropertyName("paused")] public bool? Paused { get; set; }
    [JsonPropertyName("epoch")] public long? Epoch { get; set; }
    [JsonPropertyName("region_start")] public double? RegionStart { get; set; }
    [JsonPropertyName("region_end")] public double? RegionEnd { get; set; }
    [JsonPropertyName("file_duration")] public double? FileDuration { get; set; }
    [JsonPropertyName("state")] public string? State { get; set; }
    [JsonPropertyName("text")] public string? Text { get; set; }
}

internal sealed class VizEventDto
{
    [JsonPropertyName("start_s")] public double StartS { get; set; }
    [JsonPropertyName("end_s")] public double EndS { get; set; }
    [JsonPropertyName("duration_s")] public double DurationS { get; set; }
    [JsonPropertyName("kind")] public string Kind { get; set; } = "";
}
