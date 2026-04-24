using System.Text.Json.Serialization;

namespace CwDecoderGui.Models;

public sealed class SignalProfile
{
    public static SignalProfile Empty { get; } = new();

    [JsonPropertyName("path")] public string Path { get; set; } = "";
    [JsonPropertyName("sample_rate")] public int SampleRate { get; set; }
    [JsonPropertyName("display_start_s")] public double DisplayStartSeconds { get; set; }
    [JsonPropertyName("display_end_s")] public double DisplayEndSeconds { get; set; }
    [JsonPropertyName("selection_start_s")] public double SelectionStartSeconds { get; set; }
    [JsonPropertyName("selection_end_s")] public double SelectionEndSeconds { get; set; }
    [JsonPropertyName("suggested_start_s")] public double SuggestedStartSeconds { get; set; }
    [JsonPropertyName("suggested_end_s")] public double SuggestedEndSeconds { get; set; }
    [JsonPropertyName("pitch_hz")] public double PitchHz { get; set; }
    [JsonPropertyName("threshold")] public double Threshold { get; set; }
    [JsonPropertyName("frame_step_s")] public double FrameStepSeconds { get; set; }
    [JsonPropertyName("frame_len_s")] public double FrameLengthSeconds { get; set; }
    [JsonPropertyName("points")] public SignalProfilePoint[] Points { get; set; } = [];

    [JsonIgnore]
    public bool HasData => Points.Length > 0 && DisplayEndSeconds > DisplayStartSeconds;
}

public sealed class SignalProfilePoint
{
    [JsonPropertyName("time_s")] public double TimeSeconds { get; set; }
    [JsonPropertyName("power")] public double Power { get; set; }
    [JsonPropertyName("active")] public bool Active { get; set; }
}
