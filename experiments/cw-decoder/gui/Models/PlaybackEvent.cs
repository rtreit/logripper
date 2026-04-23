using System.Text.Json.Serialization;

namespace CwDecoderGui.Models;

internal sealed class PlaybackEvent
{
    [JsonPropertyName("type")] public string Type { get; set; } = "";
    [JsonPropertyName("t")] public double T { get; set; }
    [JsonPropertyName("source")] public string? Source { get; set; }
    [JsonPropertyName("path")] public string? Path { get; set; }
    [JsonPropertyName("device")] public string? Device { get; set; }
    [JsonPropertyName("rate")] public int? Rate { get; set; }
    [JsonPropertyName("duration")] public double? Duration { get; set; }
    [JsonPropertyName("position")] public double? Position { get; set; }
}
