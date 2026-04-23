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
}
