using System.Text.Json.Serialization;

namespace CwDecoderGui.Models;

public sealed class LabelScoreRunResult
{
    [JsonPropertyName("kind")] public string Kind { get; set; } = "";
    [JsonPropertyName("labels")] public int Labels { get; set; }
    [JsonPropertyName("mode")] public string Mode { get; set; } = "";
    [JsonPropertyName("pre_roll_ms")] public int PreRollMs { get; set; }
    [JsonPropertyName("post_roll_ms")] public int PostRollMs { get; set; }
    [JsonPropertyName("baseline")] public LabelBaselineSettings Baseline { get; set; } = new();
    [JsonPropertyName("summary")] public LabelScoreSummary Summary { get; set; } = new();
    [JsonPropertyName("rows")] public LabelScoreRowResult[] Rows { get; set; } = [];
}

public sealed class LabelSweepRunResult
{
    [JsonPropertyName("kind")] public string Kind { get; set; } = "";
    [JsonPropertyName("labels")] public int Labels { get; set; }
    [JsonPropertyName("mode")] public string Mode { get; set; } = "";
    [JsonPropertyName("pre_roll_ms")] public int PreRollMs { get; set; }
    [JsonPropertyName("post_roll_ms")] public int PostRollMs { get; set; }
    [JsonPropertyName("sweep_mode")] public string SweepMode { get; set; } = "";
    [JsonPropertyName("coarse_configs")] public int CoarseConfigs { get; set; }
    [JsonPropertyName("refined_configs")] public int RefinedConfigs { get; set; }
    [JsonPropertyName("results")] public LabelSweepRowResult[] Results { get; set; } = [];
}

public sealed class LabelBaselineSettings
{
    [JsonPropertyName("window_seconds")] public double WindowSeconds { get; set; }
    [JsonPropertyName("min_window_seconds")] public double MinWindowSeconds { get; set; }
    [JsonPropertyName("decode_every_ms")] public int DecodeEveryMs { get; set; }
    [JsonPropertyName("required_confirmations")] public int RequiredConfirmations { get; set; }
}

public sealed class LabelScoreSummary
{
    [JsonPropertyName("exact")] public int Exact { get; set; }
    [JsonPropertyName("total_distance")] public int TotalDistance { get; set; }
    [JsonPropertyName("average_cer")] public double AverageCer { get; set; }
}

public sealed class LabelScoreRowResult
{
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("source")] public string Source { get; set; } = "";
    [JsonPropertyName("start_s")] public double StartSeconds { get; set; }
    [JsonPropertyName("end_s")] public double EndSeconds { get; set; }
    [JsonPropertyName("truth")] public string Truth { get; set; } = "";
    [JsonPropertyName("decoded")] public string Decoded { get; set; } = "";
    [JsonPropertyName("distance")] public int Distance { get; set; }
    [JsonPropertyName("cer")] public double Cer { get; set; }
    [JsonPropertyName("exact")] public bool Exact { get; set; }
    [JsonPropertyName("failure_class")] public string FailureClass { get; set; } = "";

    public string MatchStatus => Exact ? "EXACT" : "MISS";
}

public sealed class LabelSweepRowResult
{
    [JsonPropertyName("exact")] public int Exact { get; set; }
    [JsonPropertyName("total_distance")] public int TotalDistance { get; set; }
    [JsonPropertyName("total_cer")] public double TotalCer { get; set; }
    [JsonPropertyName("average_cer")] public double AverageCer { get; set; }
    [JsonPropertyName("worst_cer")] public double WorstCer { get; set; }
    [JsonPropertyName("window_seconds")] public double WindowSeconds { get; set; }
    [JsonPropertyName("min_window_seconds")] public double MinWindowSeconds { get; set; }
    [JsonPropertyName("decode_every_ms")] public int DecodeEveryMs { get; set; }
    [JsonPropertyName("required_confirmations")] public int RequiredConfirmations { get; set; }
}

public sealed class StrategySweepResult
{
    [JsonPropertyName("labels")] public int Labels { get; set; }
    [JsonPropertyName("strategies")] public string[] Strategies { get; set; } = [];
    [JsonPropertyName("clips")] public StrategySweepClip[] Clips { get; set; } = [];
    [JsonPropertyName("summary")] public StrategySweepStrategySummary[] Summary { get; set; } = [];
}

public sealed class StrategySweepClip
{
    [JsonPropertyName("name")] public string Name { get; set; } = "";
    [JsonPropertyName("source")] public string Source { get; set; } = "";
    [JsonPropertyName("truth")] public string Truth { get; set; } = "";
    [JsonPropertyName("truth_len")] public int TruthLen { get; set; }
    [JsonPropertyName("strategies")] public System.Collections.Generic.Dictionary<string, StrategySweepCell> Strategies { get; set; } = new();
}

public sealed class StrategySweepCell
{
    [JsonPropertyName("decoded")] public string Decoded { get; set; } = "";
    [JsonPropertyName("distance")] public int Distance { get; set; }
    [JsonPropertyName("cer")] public double Cer { get; set; }
    [JsonPropertyName("exact")] public bool Exact { get; set; }
}

public sealed class StrategySweepStrategySummary
{
    [JsonPropertyName("strategy")] public string Strategy { get; set; } = "";
    [JsonPropertyName("exact")] public int Exact { get; set; }
    [JsonPropertyName("total_distance")] public int TotalDistance { get; set; }
    [JsonPropertyName("total_truth_chars")] public int TotalTruthChars { get; set; }
    [JsonPropertyName("mean_cer")] public double MeanCer { get; set; }
    [JsonPropertyName("weighted_cer")] public double WeightedCer { get; set; }
}
