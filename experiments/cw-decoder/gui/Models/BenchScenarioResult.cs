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
    public uint CwOnsetMs { get; init; }
    public int StableN { get; init; }
    public uint? TFirstPitchUpdateMs { get; init; }
    public uint? TFirstLockedMs { get; init; }
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
    public float? LockedPitchHz { get; init; }
    public string Transcript { get; init; } = string.Empty;

    // Display helpers for the DataGrid columns.
    public string ScenarioDisplay => Scenario;
    public string LatencyDisplay => AcquisitionLatencyMs.HasValue
        ? $"{AcquisitionLatencyMs.Value / 1000.0:0.0} s"
        : "—";
    public string UptimeDisplay => LockUptimeRatio.HasValue
        ? $"{LockUptimeRatio.Value * 100.0:0.0}%"
        : "—";
    public string DropsDisplay => NPitchLostAfterLock.ToString();
    public string RelocksDisplay => NRelockCycles.ToString();
    public string LongestGapDisplay => LongestUnlockedGapMs > 0
        ? $"{LongestUnlockedGapMs / 1000.0:0.0} s"
        : "0";
    public string GhostsDisplay => FalseCharsBeforeStable.ToString();
    public string PitchDisplay => LockedPitchHz.HasValue
        ? $"{LockedPitchHz.Value:0} Hz"
        : "—";
}
