using System.Text.Json.Serialization;

namespace CwDecoderGui.Models;

public sealed class HarvestResult
{
    [JsonPropertyName("path")] public string Path { get; set; } = "";
    [JsonPropertyName("sample_rate")] public int SampleRate { get; set; }
    [JsonPropertyName("duration_s")] public double DurationSeconds { get; set; }
    [JsonPropertyName("window_s")] public double WindowSeconds { get; set; }
    [JsonPropertyName("hop_s")] public double HopSeconds { get; set; }
    [JsonPropertyName("chunk_ms")] public int ChunkMs { get; set; }
    [JsonPropertyName("needles")] public string[] Needles { get; set; } = [];
    [JsonPropertyName("candidates")] public HarvestCandidate[] Candidates { get; set; } = [];
}

public sealed class HarvestCandidate
{
    [JsonPropertyName("start_s")] public double StartSeconds { get; set; }
    [JsonPropertyName("end_s")] public double EndSeconds { get; set; }
    [JsonPropertyName("is_fallback")] public bool IsFallback { get; set; }
    [JsonPropertyName("member_count")] public int MemberCount { get; set; }
    [JsonPropertyName("shared_chars")] public int SharedChars { get; set; }
    [JsonPropertyName("strongest_copy_len")] public int StrongestCopyLength { get; set; }
    [JsonPropertyName("matched_needles")] public string[] MatchedNeedles { get; set; } = [];
    [JsonPropertyName("offline")] public HarvestDecodeSnapshot Offline { get; set; } = new();
    [JsonPropertyName("stream")] public HarvestStreamSnapshot Stream { get; set; } = new();

    public string RangeLabel => $"{StartSeconds:F2}s - {EndSeconds:F2}s";
    public string NeedlesLabel => IsFullAudio
        ? "full audio (trim with magenta handles)"
        : IsFallback
            ? "full-file fallback"
            : MatchedNeedles.Length == 0 ? "agreement" : string.Join(", ", MatchedNeedles);
    public string MemberLabel => IsFullAudio
        ? "FULL AUDIO"
        : IsFallback
            ? "fallback"
            : MemberCount <= 1 ? "1 window" : $"{MemberCount} windows";

    [JsonIgnore]
    public bool IsFullAudio { get; set; }
}

public class HarvestDecodeSnapshot
{
    [JsonPropertyName("text")] public string Text { get; set; } = "";
    [JsonPropertyName("pitch_hz")] public double? PitchHz { get; set; }
    [JsonPropertyName("wpm")] public double? Wpm { get; set; }
}

public sealed class HarvestStreamSnapshot : HarvestDecodeSnapshot
{
    [JsonPropertyName("threshold")] public double Threshold { get; set; }
}
