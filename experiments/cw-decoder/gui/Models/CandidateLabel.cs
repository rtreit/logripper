using System.Text.Json.Serialization;

namespace CwDecoderGui.Models;

internal sealed class CandidateLabel
{
    public const string ExactWindowScope = "exact_window";

    [JsonPropertyName("source")] public string Source { get; set; } = "";
    [JsonPropertyName("start_s")] public double StartSeconds { get; set; }
    [JsonPropertyName("end_s")] public double EndSeconds { get; set; }
    [JsonPropertyName("harvest_start_s")] public double? HarvestStartSeconds { get; set; }
    [JsonPropertyName("harvest_end_s")] public double? HarvestEndSeconds { get; set; }
    [JsonPropertyName("label_scope")] public string LabelScope { get; set; } = ExactWindowScope;
    [JsonPropertyName("correct_copy")] public string CorrectCopy { get; set; } = "";
    [JsonPropertyName("clip_start")] public bool ClipStart { get; set; }
    [JsonPropertyName("clip_end")] public bool ClipEnd { get; set; }
    [JsonPropertyName("needles")] public string[] Needles { get; set; } = [];
    [JsonPropertyName("offline_text")] public string OfflineText { get; set; } = "";
    [JsonPropertyName("stream_text")] public string StreamText { get; set; } = "";
    [JsonPropertyName("offline_pitch_hz")] public double? OfflinePitchHz { get; set; }
    [JsonPropertyName("stream_pitch_hz")] public double? StreamPitchHz { get; set; }
    [JsonPropertyName("offline_wpm")] public double? OfflineWpm { get; set; }
    [JsonPropertyName("stream_wpm")] public double? StreamWpm { get; set; }
    [JsonPropertyName("saved_at_utc")] public string SavedAtUtc { get; set; } = "";
}
