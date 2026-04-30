using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using CwDecoderGui.Models;

namespace CwDecoderGui.Services;

/// <summary>
/// Runs the Rust <c>cw-decoder bench-latency --json</c> subcommand and
/// streams parsed scenario results back to the caller. Each line of the
/// child's stdout is checked for a JSON object whose <c>type</c> field is
/// <c>bench_result</c>; parsed rows are pushed through
/// <paramref name="onScenario"/>. Non-JSON / non-result lines (the table,
/// header, aggregate, etc.) are aggregated and returned at the end so the
/// VM can show the full transcript in a "raw output" pane.
/// </summary>
internal static class CwBenchRunner
{
    public sealed class Options
    {
        public string Label { get; set; } = "default";
        public int StableN { get; set; } = 5;
        public int ChunkMs { get; set; } = 100;
        public int SynthSampleRate { get; set; } = 16000;
        // null/empty => synthetic suite.
        public string? FromFile { get; set; }
        public uint CwOnsetMs { get; set; }
        public string? Truth { get; set; }
        // Decoder knobs (null => leave default).
        public float? Purity { get; set; }
        public int? WideBins { get; set; }
        public bool DisableAutoThreshold { get; set; }
        public float? ForcePitchHz { get; set; }
        public bool Foundation { get; set; }
    }

    public sealed class RunResult
    {
        public IReadOnlyList<BenchScenarioResult> Scenarios { get; init; } =
            Array.Empty<BenchScenarioResult>();
        public string RawOutput { get; init; } = string.Empty;
        public int ExitCode { get; init; }
    }

    public static async Task<RunResult> RunAsync(
        Options opts,
        Action<BenchScenarioResult> onScenario,
        Action<string>? onLog,
        CancellationToken ct)
    {
        var exe = LocateBinary()
            ?? throw new InvalidOperationException(
                "Could not locate cw-decoder.exe. Run `cargo build --release` in experiments/cw-decoder first.");

        var psi = new ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(exe)!,
        };
        psi.ArgumentList.Add("bench-latency");
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--label");
        psi.ArgumentList.Add(opts.Label);
        psi.ArgumentList.Add("--stable-n");
        psi.ArgumentList.Add(opts.StableN.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--chunk-ms");
        psi.ArgumentList.Add(opts.ChunkMs.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--synth-rate");
        psi.ArgumentList.Add(opts.SynthSampleRate.ToString(CultureInfo.InvariantCulture));

        if (!string.IsNullOrWhiteSpace(opts.FromFile))
        {
            psi.ArgumentList.Add("--from-file");
            psi.ArgumentList.Add(opts.FromFile);
            psi.ArgumentList.Add("--cw-onset-ms");
            psi.ArgumentList.Add(opts.CwOnsetMs.ToString(CultureInfo.InvariantCulture));
            if (!string.IsNullOrWhiteSpace(opts.Truth))
            {
                psi.ArgumentList.Add("--truth");
                psi.ArgumentList.Add(opts.Truth);
            }
        }

        if (opts.Purity is float purity)
        {
            psi.ArgumentList.Add("--purity");
            psi.ArgumentList.Add(purity.ToString(CultureInfo.InvariantCulture));
        }
        if (opts.WideBins is int wb)
        {
            psi.ArgumentList.Add("--wide-bins");
            psi.ArgumentList.Add(wb.ToString(CultureInfo.InvariantCulture));
        }
        if (opts.DisableAutoThreshold)
        {
            psi.ArgumentList.Add("--no-auto-threshold");
        }
        if (opts.ForcePitchHz is float fp && fp > 0f)
        {
            psi.ArgumentList.Add("--force-pitch-hz");
            psi.ArgumentList.Add(fp.ToString(CultureInfo.InvariantCulture));
        }
        if (opts.Foundation)
        {
            psi.ArgumentList.Add("--foundation");
        }

        using var process = Process.Start(psi)
            ?? throw new InvalidOperationException("Failed to start cw-decoder bench-latency.");

        using var registration = ct.Register(() =>
        {
            try
            {
                if (!process.HasExited)
                {
                    process.Kill(entireProcessTree: true);
                }
            }
            catch
            {
            }
        });

        var rows = new List<BenchScenarioResult>();
        var raw = new StringBuilder();

        var stderrTask = Task.Run(async () =>
        {
            string? line;
            while ((line = await process.StandardError.ReadLineAsync().ConfigureAwait(false)) is not null)
            {
                onLog?.Invoke(line);
            }
        });

        string? stdoutLine;
        while ((stdoutLine = await process.StandardOutput.ReadLineAsync().ConfigureAwait(false)) is not null)
        {
            raw.AppendLine(stdoutLine);
            var trimmed = stdoutLine.TrimStart();
            if (trimmed.StartsWith('{') && trimmed.Contains("\"type\"", StringComparison.Ordinal))
            {
                if (TryParseRow(stdoutLine, out var row))
                {
                    rows.Add(row);
                    onScenario(row);
                    continue;
                }
            }
            onLog?.Invoke(stdoutLine);
        }

        await stderrTask.ConfigureAwait(false);
        await process.WaitForExitAsync(ct).ConfigureAwait(false);

        return new RunResult
        {
            Scenarios = rows,
            RawOutput = raw.ToString(),
            ExitCode = process.ExitCode,
        };
    }

    private static bool TryParseRow(string json, out BenchScenarioResult row)
    {
        row = default!;
        try
        {
            using var doc = JsonDocument.Parse(json);
            var r = doc.RootElement;
            if (r.ValueKind != JsonValueKind.Object) return false;
            if (!r.TryGetProperty("type", out var t) || t.GetString() != "bench_result")
            {
                return false;
            }

            row = new BenchScenarioResult
            {
                Label = GetString(r, "label") ?? string.Empty,
                Scenario = GetString(r, "scenario") ?? string.Empty,
                CwOnsetMs = GetUInt32(r, "cw_onset_ms") ?? 0u,
                StableN = (int)(GetUInt32(r, "stable_n") ?? 0u),
                TFirstPitchUpdateMs = GetUInt32(r, "t_first_pitch_update_ms"),
                TFirstLockedMs = GetUInt32(r, "t_first_locked_ms"),
                TFirstCharMs = GetUInt32(r, "t_first_char_ms"),
                TFirstCorrectCharMs = GetUInt32(r, "t_first_correct_char_ms"),
                TStableNCorrectMs = GetUInt32(r, "t_stable_n_correct_ms"),
                AcquisitionLatencyMs = GetInt64(r, "acquisition_latency_ms"),
                FalseCharsBeforeStable = (int)(GetUInt32(r, "false_chars_before_stable") ?? 0u),
                NPitchLostAfterLock = (int)(GetUInt32(r, "n_pitch_lost_after_lock") ?? 0u),
                NRelockCycles = (int)(GetUInt32(r, "n_relock_cycles") ?? 0u),
                LockUptimeRatio = GetSingle(r, "lock_uptime_ratio"),
                LongestUnlockedGapMs = GetUInt32(r, "longest_unlocked_gap_ms") ?? 0u,
                TotalUnlockedMsAfterLock = GetUInt32(r, "total_unlocked_ms_after_lock") ?? 0u,
                LockedPitchHz = GetSingle(r, "locked_pitch_hz"),
                Transcript = GetString(r, "transcript") ?? string.Empty,
            };
            return true;
        }
        catch
        {
            return false;
        }
    }

    private static string? GetString(JsonElement r, string name) =>
        r.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String
            ? v.GetString()
            : null;

    private static uint? GetUInt32(JsonElement r, string name) =>
        r.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.Number
            ? (uint)v.GetInt64()
            : null;

    private static long? GetInt64(JsonElement r, string name) =>
        r.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.Number
            ? v.GetInt64()
            : null;

    private static float? GetSingle(JsonElement r, string name) =>
        r.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.Number
            ? (float)v.GetDouble()
            : null;

    private static string? LocateBinary()
    {
        var env = Environment.GetEnvironmentVariable("CW_DECODER_EXE");
        if (!string.IsNullOrWhiteSpace(env) && File.Exists(env)) return env;

        var exeName = OperatingSystem.IsWindows() ? "cw-decoder.exe" : "cw-decoder";
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        for (int i = 0; dir is not null && i < 8; i++, dir = dir.Parent)
        {
            string[] candidates =
            {
                Path.Combine(dir.FullName, "target", "release", exeName),
                Path.Combine(dir.FullName, "target", "debug", exeName),
                Path.Combine(dir.FullName, "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "cw-decoder", "target", "debug", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "debug", exeName),
            };
            FileInfo? newest = null;
            foreach (var path in candidates)
            {
                if (!File.Exists(path)) continue;
                var info = new FileInfo(path);
                if (newest is null || info.LastWriteTimeUtc > newest.LastWriteTimeUtc)
                {
                    newest = info;
                }
            }
            if (newest is not null) return newest.FullName;
        }
        return null;
    }
}
