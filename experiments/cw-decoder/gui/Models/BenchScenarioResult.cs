namespace CwDecoderGui.Models;

/// <summary>
/// One row of cw-decoder bench-latency NDJSON output, deserialized into a
/// flat record so the GUI can bind directly. Numeric fields that the
/// underlying benchmark may report as `null` are nullable here too.
/// </summary>
public sealed class BenchScenarioResult
{
    public string Label { get; init; } = string.Empty;
    public string Scenario { get; init; } = string.Empty;
    public string DecoderPath { get; init; } = string.Empty;
    public uint CwOnsetMs { get; init; }
    public int StableN { get; init; }
    public uint? TFirstPitchUpdateMs { get; init; }
    public uint? TFirstLockedMs { get; init; }
    public uint? TFirstFoundationLockMs { get; init; }
    public uint? TFirstCharMs { get; init; }
    public uint? TFirstCorrectCharMs { get; init; }
    public uint? TStableNCorrectMs { get; init; }
    public long? AcquisitionLatencyMs { get; init; }
    public int FalseCharsBeforeStable { get; init; }
    public int NPitchLostAfterLock { get; init; }
    public int NRelockCycles { get; init; }
    public float? LockUptimeRatio { get; init; }
    public uint LongestUnlockedGapMs { get; init; }
    public uint TotalUnlockedMsAfterLock { get; init; }
    public int QualityGateDrops { get; init; }
    public int QualityGateRecoveries { get; init; }
    public float? QualityGateUptimeRatio { get; init; }
    public uint LongestQualityGateClosedMs { get; init; }
    public float? LockedPitchHz { get; init; }
    public float? FinalPitchHz { get; init; }
    public float? FinalLockedWpm { get; init; }
    public float? CerVsTruth { get; init; }
    public string Transcript { get; init; } = string.Empty;

    private bool IsFoundation => string.Equals(DecoderPath, "foundation", System.StringComparison.OrdinalIgnoreCase);

    // Display helpers for the DataGrid columns.
    public string ScenarioDisplay => Scenario;
    public string DecoderDisplay => string.IsNullOrEmpty(DecoderPath) ? "—" : DecoderPath;
    public string LatencyDisplay => AcquisitionLatencyMs.HasValue
        ? $"{AcquisitionLatencyMs.Value / 1000.0:0.0} s"
        : "—";
    public string UptimeDisplay
    {
        get
        {
            var ratio = IsFoundation ? QualityGateUptimeRatio : LockUptimeRatio;
            return ratio.HasValue ? $"{ratio.Value * 100.0:0.0}%" : "—";
        }
    }
    public string DropsDisplay => (IsFoundation ? QualityGateDrops : NPitchLostAfterLock).ToString();
    public string RelocksDisplay => (IsFoundation ? QualityGateRecoveries : NRelockCycles).ToString();
    public string LongestGapDisplay
    {
        get
        {
            var ms = IsFoundation ? LongestQualityGateClosedMs : LongestUnlockedGapMs;
            return ms > 0 ? $"{ms / 1000.0:0.0} s" : "0";
        }
    }
    public string GhostsDisplay => FalseCharsBeforeStable.ToString();
    public string PitchDisplay
    {
        get
        {
            var p = LockedPitchHz ?? FinalPitchHz;
            return p.HasValue ? $"{p.Value:0} Hz" : "—";
        }
    }
    public string CerDisplay => CerVsTruth.HasValue ? $"{CerVsTruth.Value:0.000}" : "—";
}
