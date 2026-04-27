using System;
using System.Collections.Generic;
using System.Globalization;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using CwDecoderGui.Models;

namespace CwDecoderGui.Services;

/// <summary>
/// Spawns the Rust <c>cw-decoder</c> binary in <c>--json</c> mode and surfaces
/// each parsed event via <see cref="EventReceived"/>. Locates the binary by
/// walking up from the current AppContext.BaseDirectory until it finds the
/// experiments/cw-decoder/target/release executable, so this works whether
/// the GUI is launched from Visual Studio, `dotnet run`, or a published bundle.
/// </summary>
internal sealed class CwDecoderProcess : IDisposable
{
    public event Action<DecoderEvent>? EventReceived;
    public event Action<string>? StderrLine;
    public event Action<int>? Exited;

    private Process? _proc;
    private CancellationTokenSource? _cts;

    public bool IsRunning => _proc is { HasExited: false };

    /// <summary>List input devices via the Rust binary.</summary>
    public static string[] ListDevices()
    {
        var (inputs, _) = ListAllDevices();
        return inputs;
    }

    /// Returns (inputs, outputs). Outputs are usable as --loopback targets.
    public static (string[] Inputs, string[] Outputs) ListAllDevices()
    {
        var exe = LocateBinary();
        if (exe is null) return (Array.Empty<string>(), Array.Empty<string>());
        try
        {
            var psi = new ProcessStartInfo(exe, "devices")
            {
                RedirectStandardOutput = true,
                UseShellExecute = false,
                CreateNoWindow = true,
            };
            using var p = Process.Start(psi);
            if (p is null) return (Array.Empty<string>(), Array.Empty<string>());
            var text = p.StandardOutput.ReadToEnd();
            p.WaitForExit(3000);
            var lines = text.Split('\n', StringSplitOptions.RemoveEmptyEntries);
            var inputs = new System.Collections.Generic.List<string>();
            var outputs = new System.Collections.Generic.List<string>();
            int section = 0; // 0 = none, 1 = inputs, 2 = outputs
            foreach (var raw in lines)
            {
                var line = raw.TrimEnd('\r');
                var trimmed = line.TrimStart();
                if (trimmed.StartsWith("Input devices", StringComparison.OrdinalIgnoreCase))
                {
                    section = 1;
                    continue;
                }
                if (trimmed.StartsWith("Output devices", StringComparison.OrdinalIgnoreCase))
                {
                    section = 2;
                    continue;
                }
                if (trimmed.StartsWith("- "))
                {
                    var name = trimmed[2..].Trim();
                    if (section == 1) inputs.Add(name);
                    else if (section == 2) outputs.Add(name);
                }
            }
            return (inputs.ToArray(), outputs.ToArray());
        }
        catch { return (Array.Empty<string>(), Array.Empty<string>()); }
    }

    public void StartLive(string? device, DecoderConfig cfg, BaselineDecoderConfig baselineCfg, bool useBaseline, string? recordPath = null, bool loopback = false, bool useV2 = false, double pinWpm = 0)
    {
        Stop();
        // v2 (whole-buffer ditdah, default in production GUI): keep an
        // append-only audio buffer; re-decode the entire buffer with
        // pristine ditdah every 5s; emit a `transcript` event that
        // REPLACES the prior text wholesale. Empirical CER ~0.06 on
        // training-set-a vs 0.83+ for the rolling-window baseline.
        var ic = CultureInfo.InvariantCulture;
        var v2Args = "stream-live-v2 --json --stdin-control --decode-every-ms 5000";
        if (pinWpm > 0)
        {
            v2Args += $" --pin-wpm {pinWpm.ToString(ic)}";
        }
        // Live baseline mirrors the offline replay configuration: large
        // rolling window so consecutive snapshots overlap heavily, slow
        // enough cadence that the prefix-stabilizer's confirmation logic
        // can lock in committed text without producing duplications, but
        // fast enough that operators see chars within ~1.5s of being sent.
        var liveBaselineArgs = "--window 20 --min-window 2 --decode-every-ms 500 --confirmations 3";
        string args;
        if (useV2)
        {
            args = v2Args;
        }
        else if (useBaseline)
        {
            args = $"stream-live-ditdah --json --chunk-ms 50 {liveBaselineArgs}";
        }
        else
        {
            args = $"stream-live --json --stdin-control {cfg.ToCliArgs()}";
        }
        if (!string.IsNullOrWhiteSpace(device)) args += $" --device \"{device}\"";
        if (!string.IsNullOrWhiteSpace(recordPath)) args += $" --record \"{recordPath}\"";
        // Loopback applies to the custom streaming decoder and v2
        // (the legacy ditdah baseline path doesn't accept --loopback).
        if (loopback && !useBaseline) args += " --loopback";
        Spawn(args);
    }

    public void StartFile(string path, bool realtime, DecoderConfig cfg, BaselineDecoderConfig baselineCfg, bool useBaseline)
    {
        Stop();
        var args = useBaseline
            ? $"stream-file-ditdah --json --chunk-ms 50 {baselineCfg.ToCliArgs()} \"{path}\""
            : $"stream-file --json --stdin-control {cfg.ToCliArgs()} \"{path}\"";
        if (realtime) args += " --realtime";
        Spawn(args);
    }

    /// <summary>
    /// Start the lockstep decode-and-play subcommand. Audio output and
    /// the streaming decoder run in the same process from a single
    /// monotonic clock, so the operator hears exactly what is being
    /// decoded. Optional region trim and runtime control are applied
    /// via stdin (Pause/Resume/Seek).
    /// </summary>
    public void StartDecodeAndPlay(string path, double startSeconds, double endSeconds, DecoderConfig cfg)
    {
        Stop();
        var ic = CultureInfo.InvariantCulture;
        var args = $"decode-and-play --json --stdin-control {cfg.ToCliArgs()}"
            + $" --start {startSeconds.ToString(ic)}"
            + $" --end {endSeconds.ToString(ic)}"
            + $" \"{path}\"";
        Spawn(args);
    }

    /// <summary>Pause the running decode-and-play process. No-op otherwise.</summary>
    public void Pause() => SendCommand("{\"cmd\":\"pause\"}");

    /// <summary>Resume the running decode-and-play process. No-op otherwise.</summary>
    public void Resume() => SendCommand("{\"cmd\":\"resume\"}");

    /// <summary>
    /// Seek the running decode-and-play process to <paramref name="positionSeconds"/>
    /// (region-relative). The Rust side resets the decoder state at the
    /// new position so prior pitch lock / threshold history can't bleed
    /// across the seek.
    /// </summary>
    public void Seek(double positionSeconds)
    {
        var ic = CultureInfo.InvariantCulture;
        SendCommand($"{{\"cmd\":\"seek\",\"position\":{positionSeconds.ToString(ic)}}}");
    }

    private void SendCommand(string ndjsonLine)
    {
        var p = _proc;
        if (p is null || p.HasExited) return;
        try
        {
            p.StandardInput.WriteLine(ndjsonLine);
            p.StandardInput.Flush();
        }
        catch { /* best effort: process may be in shutdown */ }
    }

    public async Task<HarvestResult> HarvestFileAsync(
        string path,
        double windowSeconds,
        double hopSeconds,
        int chunkMs,
        int top,
        int minSharedChars,
        string[] needles,
        DecoderConfig cfg,
        Action<int, int, double, double>? onProgress = null,
        CancellationToken ct = default)
    {
        var psi = CreateBaseStartInfo();
        psi.ArgumentList.Add("harvest-file");
        psi.ArgumentList.Add(path);
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--window");
        psi.ArgumentList.Add(F(windowSeconds));
        psi.ArgumentList.Add("--hop");
        psi.ArgumentList.Add(F(hopSeconds));
        psi.ArgumentList.Add("--chunk-ms");
        psi.ArgumentList.Add(chunkMs.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--top");
        psi.ArgumentList.Add(top.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--min-shared-chars");
        psi.ArgumentList.Add(minSharedChars.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--min-snr-db");
        psi.ArgumentList.Add(F(cfg.MinSnrDb));
        psi.ArgumentList.Add("--pitch-min-snr-db");
        psi.ArgumentList.Add(F(cfg.PitchMinSnrDb));
        psi.ArgumentList.Add("--threshold-scale");
        psi.ArgumentList.Add(F(cfg.ThresholdScale));
        foreach (var needle in needles)
        {
            psi.ArgumentList.Add("--needle");
            psi.ArgumentList.Add(needle);
        }

        using var process = Process.Start(psi)
            ?? throw new InvalidOperationException("Failed to start cw-decoder.");
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

        var stdoutTask = process.StandardOutput.ReadToEndAsync();
        var stderrTask = Task.Run(async () =>
        {
            var stderr = new StringBuilder();
            string? line;
            while ((line = await process.StandardError.ReadLineAsync().ConfigureAwait(false)) is not null)
            {
                if (TryParseHarvestProgress(line, out var completed, out var total, out var startSeconds, out var endSeconds))
                {
                    onProgress?.Invoke(completed, total, startSeconds, endSeconds);
                    continue;
                }

                if (stderr.Length > 0)
                {
                    stderr.AppendLine();
                }
                stderr.Append(line);
            }

            return stderr.ToString();
        }, ct);

        await process.WaitForExitAsync(ct).ConfigureAwait(false);
        var stdout = await stdoutTask.ConfigureAwait(false);
        var stderr = await stderrTask.ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                string.IsNullOrWhiteSpace(stderr) ? $"cw-decoder exited with code {process.ExitCode}." : stderr.Trim());
        }

        return JsonSerializer.Deserialize<HarvestResult>(stdout)
            ?? throw new InvalidOperationException("Failed to parse harvest-file output.");
    }

    public async Task<string> RenderPreviewAsync(
        string path,
        double startSeconds,
        double windowSeconds,
        double slowdown,
        CancellationToken ct = default)
    {
        var previewDir = Path.Combine(Path.GetTempPath(), "cw-decoder-preview");
        Directory.CreateDirectory(previewDir);
        var output = Path.Combine(
            previewDir,
            $"{Path.GetFileNameWithoutExtension(path)}_{startSeconds:0000.000}_{DateTime.UtcNow:yyyyMMddHHmmssfff}.wav");

        var psi = CreateBaseStartInfo();
        psi.ArgumentList.Add("preview-window");
        psi.ArgumentList.Add(path);
        psi.ArgumentList.Add("--start");
        psi.ArgumentList.Add(F(startSeconds));
        psi.ArgumentList.Add("--window");
        psi.ArgumentList.Add(F(windowSeconds));
        psi.ArgumentList.Add("--slowdown");
        psi.ArgumentList.Add(F(slowdown));
        psi.ArgumentList.Add("--output");
        psi.ArgumentList.Add(output);

        await RunOneShotAsync(psi, ct).ConfigureAwait(false);
        return output;
    }

    public async Task<SignalProfile> LoadSignalProfileAsync(
        string path,
        double startSeconds,
        double endSeconds,
        double? pitchHz,
        double? wpm,
        CancellationToken ct = default)
    {
        var psi = CreateBaseStartInfo();
        psi.ArgumentList.Add("profile-window");
        psi.ArgumentList.Add(path);
        psi.ArgumentList.Add("--start");
        psi.ArgumentList.Add(F(startSeconds));
        psi.ArgumentList.Add("--end");
        psi.ArgumentList.Add(F(endSeconds));
        if (pitchHz is double pitchValue && pitchValue > 0)
        {
            psi.ArgumentList.Add("--pitch-hz");
            psi.ArgumentList.Add(F(pitchValue));
        }
        if (wpm is double wpmValue && wpmValue > 0)
        {
            psi.ArgumentList.Add("--wpm");
            psi.ArgumentList.Add(F(wpmValue));
        }

        var stdout = await RunOneShotAsync(psi, ct).ConfigureAwait(false);
        return JsonSerializer.Deserialize<SignalProfile>(stdout)
            ?? throw new InvalidOperationException("Failed to parse profile-window output.");
    }

    public async Task<LabelScoreRunResult> RunLabelScoreAsync(
        bool allLabels,
        IReadOnlyList<string>? labelPaths,
        bool fullStreamMode,
        int preRollMs,
        int postRollMs,
        double windowSeconds,
        double minWindowSeconds,
        int decodeEveryMs,
        int confirmations,
        DecoderConfig cfg,
        CancellationToken ct = default)
    {
        var psi = CreateEvalStartInfo();
        AddLabelSelectionArguments(psi, allLabels, labelPaths);
        AddLabelScoreModeArguments(psi, fullStreamMode, preRollMs, postRollMs);
        AddStreamingExperimentArguments(psi, cfg);
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--window");
        psi.ArgumentList.Add(F(windowSeconds));
        psi.ArgumentList.Add("--min-window");
        psi.ArgumentList.Add(F(minWindowSeconds));
        psi.ArgumentList.Add("--decode-every-ms");
        psi.ArgumentList.Add(decodeEveryMs.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--confirmations");
        psi.ArgumentList.Add(confirmations.ToString(CultureInfo.InvariantCulture));
        var stdout = await RunOneShotAsync(psi, ct).ConfigureAwait(false);
        return JsonSerializer.Deserialize<LabelScoreRunResult>(stdout)
            ?? throw new InvalidOperationException("Failed to parse label score output.");
    }

    public async Task<LabelSweepRunResult> RunLabelSweepAsync(
        bool allLabels,
        IReadOnlyList<string>? labelPaths,
        bool fullStreamMode,
        int preRollMs,
        int postRollMs,
        bool wideSweep,
        int top,
        CancellationToken ct = default)
    {
        var psi = CreateEvalStartInfo();
        AddLabelSelectionArguments(psi, allLabels, labelPaths);
        AddLabelScoreModeArguments(psi, fullStreamMode, preRollMs, postRollMs);
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--sweep-ditdah");
        psi.ArgumentList.Add("--top");
        psi.ArgumentList.Add(top.ToString(CultureInfo.InvariantCulture));
        if (wideSweep)
        {
            psi.ArgumentList.Add("--wide-sweep");
        }

        var stdout = await RunOneShotAsync(psi, ct).ConfigureAwait(false);
        return JsonSerializer.Deserialize<LabelSweepRunResult>(stdout)
            ?? throw new InvalidOperationException("Failed to parse label sweep output.");
    }

    public async Task<StrategySweepResult> RunStrategySweepAsync(
        bool allLabels,
        IReadOnlyList<string>? labelPaths,
        IReadOnlyList<string>? strategies,
        CancellationToken ct = default)
    {
        var psi = CreateEvalStartInfo();
        AddLabelSelectionArguments(psi, allLabels, labelPaths);
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--strategy-sweep");
        if (strategies is { Count: > 0 })
        {
            psi.ArgumentList.Add("--strategies");
            psi.ArgumentList.Add(string.Join(",", strategies));
        }

        var stdout = await RunOneShotAsync(psi, ct).ConfigureAwait(false);
        return JsonSerializer.Deserialize<StrategySweepResult>(stdout)
            ?? throw new InvalidOperationException("Failed to parse strategy sweep output.");
    }

    public static string[] ListAvailableLabelFiles()
    {
        try
        {
            return Directory.EnumerateFiles(LocateLabelCorpusDirectory(), "*.labels.jsonl")
                .OrderBy(Path.GetFileName, StringComparer.OrdinalIgnoreCase)
                .ToArray();
        }
        catch
        {
            return [];
        }
    }

    public static void OpenPreview(string path)
    {
        var psi = new ProcessStartInfo(path) { UseShellExecute = true };
        Process.Start(psi);
    }

    /// <summary>Send a runtime config update to the running decoder via stdin.
    /// No-op if no decoder is running. Safe to call from any thread.</summary>
    public void SendConfig(DecoderConfig cfg)
    {
        var p = _proc;
        if (p is null || p.HasExited) return;
        try
        {
            p.StandardInput.WriteLine(cfg.ToJsonCommand());
            p.StandardInput.Flush();
        }
        catch { /* best effort: process may be in shutdown */ }
    }

    public void Stop()
    {
        try
        {
            _cts?.Cancel();
            if (_proc is { HasExited: false } proc)
            {
                // Signal the Rust child to shut down gracefully:
                //   1) Write "stop\n" — the watcher thread's blocking read
                //      returns immediately and flips the stop atomic.
                //   2) Close stdin for EOF as a fallback.
                //   3) WaitForExit up to 3s so Drop runs on LiveCapture and
                //      hound finalizes the WAV header. Without this the
                //      recording has RIFF size=0 / missing data chunk and
                //      Replay & Score gets "(empty)".
                // The Rust path finalizes the WAV BEFORE emitting the end
                // event, so even if we fall through to Kill the header is
                // already valid.
                try
                {
                    proc.StandardInput.WriteLine("stop");
                    proc.StandardInput.Flush();
                }
                catch { /* best effort */ }
                try { proc.StandardInput.Close(); } catch { /* best effort */ }
                if (!proc.WaitForExit(3000))
                {
                    try { proc.Kill(entireProcessTree: true); } catch { /* best effort */ }
                }
            }
        }
        catch { /* ignored */ }
        _proc = null;
        _cts = null;
    }

    public void Dispose() => Stop();

    private void Spawn(string args)
    {
        var psi = CreateBaseStartInfo(args);
        psi.RedirectStandardInput = true;
        var p = Process.Start(psi) ?? throw new InvalidOperationException("Failed to start cw-decoder.");
        _proc = p;
        _cts = new CancellationTokenSource();
        _ = Task.Run(() => PumpStdoutAsync(p, _cts.Token));
        _ = Task.Run(() => PumpStderrAsync(p, _cts.Token));
        _ = Task.Run(() =>
        {
            try { p.WaitForExit(); } catch { /* ignored */ }
            Exited?.Invoke(p.ExitCode);
        });
    }

    private async Task PumpStdoutAsync(Process p, CancellationToken ct)
    {
        try
        {
            string? line;
            while (!ct.IsCancellationRequested && (line = await p.StandardOutput.ReadLineAsync().ConfigureAwait(false)) != null)
            {
                if (string.IsNullOrWhiteSpace(line)) continue;
                DecoderEvent? ev = null;
                try { ev = JsonSerializer.Deserialize<DecoderEvent>(line); }
                catch (JsonException) { /* ignore non-JSON lines, e.g. stray prints */ }
                if (ev is not null) EventReceived?.Invoke(ev);
            }
        }
        catch (OperationCanceledException) { /* expected on stop */ }
        catch (Exception ex) { StderrLine?.Invoke($"[gui] stdout pump error: {ex.Message}"); }
    }

    private async Task PumpStderrAsync(Process p, CancellationToken ct)
    {
        try
        {
            string? line;
            while (!ct.IsCancellationRequested && (line = await p.StandardError.ReadLineAsync().ConfigureAwait(false)) != null)
            {
                StderrLine?.Invoke(line);
            }
        }
        catch (OperationCanceledException) { }
        catch (Exception ex) { StderrLine?.Invoke($"[gui] stderr pump error: {ex.Message}"); }
    }

    private static string? LocateBinary()
    {
        // 1) env override
        var env = Environment.GetEnvironmentVariable("CW_DECODER_EXE");
        if (!string.IsNullOrWhiteSpace(env) && File.Exists(env)) return env;

        var exeName = OperatingSystem.IsWindows() ? "cw-decoder.exe" : "cw-decoder";
        // 2) walk up from BaseDirectory looking for experiments/cw-decoder/target/{release,debug}
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        for (int i = 0; dir is not null && i < 8; i++, dir = dir.Parent)
        {
            var candidates = new[]
            {
                Path.Combine(dir.FullName, "target", "release", exeName),
                Path.Combine(dir.FullName, "target", "debug", exeName),
                Path.Combine(dir.FullName, "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "cw-decoder", "target", "debug", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "debug", exeName),
            };

            var newest = candidates
                .Where(File.Exists)
                .Select(path => new FileInfo(path))
                .OrderByDescending(info => info.LastWriteTimeUtc)
                .FirstOrDefault();
            if (newest is not null) return newest.FullName;
        }
        return null;
    }

    private static string LocateEvalBinary()
    {
        var decoderExe = LocateBinary() ?? throw new InvalidOperationException(
            "Could not locate cw-decoder.exe. Run `cargo build --release` in experiments/cw-decoder first.");
        var evalName = OperatingSystem.IsWindows() ? "eval.exe" : "eval";
        var evalPath = Path.Combine(Path.GetDirectoryName(decoderExe)!, evalName);
        if (File.Exists(evalPath))
        {
            return evalPath;
        }

        throw new InvalidOperationException(
            "Could not locate eval.exe. Run `cargo build --release --bins` in experiments/cw-decoder first.");
    }

    private static ProcessStartInfo CreateBaseStartInfo(string? args = null)
    {
        var exe = LocateBinary() ?? throw new InvalidOperationException(
            "Could not locate cw-decoder.exe. Run `cargo build --release` in experiments/cw-decoder first.");
        return CreateStartInfo(exe, args);
    }

    private static ProcessStartInfo CreateEvalStartInfo(string? args = null)
    {
        var exe = LocateEvalBinary();
        return CreateStartInfo(exe, args);
    }

    private static ProcessStartInfo CreateStartInfo(string exe, string? args = null)
    {
        var psi = string.IsNullOrWhiteSpace(args)
            ? new ProcessStartInfo(exe)
            : new ProcessStartInfo(exe, args);
        psi.RedirectStandardOutput = true;
        psi.RedirectStandardError = true;
        psi.UseShellExecute = false;
        psi.CreateNoWindow = true;
        psi.WorkingDirectory = Path.GetDirectoryName(exe)!;
        return psi;
    }

    private static void AddLabelSelectionArguments(
        ProcessStartInfo psi,
        bool allLabels,
        IReadOnlyList<string>? labelPaths)
    {
        if (allLabels)
        {
            psi.ArgumentList.Add("--labels-dir");
            psi.ArgumentList.Add(LocateLabelCorpusDirectory());
            return;
        }

        if (labelPaths is null || labelPaths.Count == 0)
        {
            throw new InvalidOperationException("Pick at least one label file, or enable all-labels.");
        }

        foreach (var labelPath in labelPaths)
        {
            psi.ArgumentList.Add("--labels");
            psi.ArgumentList.Add(labelPath);
        }
    }

    private static string LocateLabelCorpusDirectory()
    {
        var decoderExe = LocateBinary() ?? throw new InvalidOperationException(
            "Could not locate cw-decoder.exe. Run `cargo build --release` in experiments/cw-decoder first.");
        var dir = new DirectoryInfo(Path.GetDirectoryName(decoderExe)!);
        for (int i = 0; dir is not null && i < 8; i++, dir = dir.Parent)
        {
            var candidate = Path.Combine(dir.FullName, "data", "cw-samples");
            if (Directory.Exists(candidate))
            {
                return candidate;
            }
        }

        throw new InvalidOperationException(
            "Could not locate the label corpus directory. Expected data\\cw-samples somewhere above the decoder build output.");
    }

    private static void AddLabelScoreModeArguments(
        ProcessStartInfo psi,
        bool fullStreamMode,
        int preRollMs,
        int postRollMs)
    {
        if (fullStreamMode)
        {
            psi.ArgumentList.Add("--mode");
            psi.ArgumentList.Add("full-stream");
        }

        if (preRollMs > 0)
        {
            psi.ArgumentList.Add("--pre-roll-ms");
            psi.ArgumentList.Add(preRollMs.ToString(CultureInfo.InvariantCulture));
        }

        if (postRollMs > 0)
        {
            psi.ArgumentList.Add("--post-roll-ms");
            psi.ArgumentList.Add(postRollMs.ToString(CultureInfo.InvariantCulture));
        }
    }

    private static void AddStreamingExperimentArguments(ProcessStartInfo psi, DecoderConfig cfg)
    {
        psi.ArgumentList.Add("--min-snr-db");
        psi.ArgumentList.Add(cfg.MinSnrDb.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--pitch-min-snr-db");
        psi.ArgumentList.Add(cfg.PitchMinSnrDb.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--threshold-scale");
        psi.ArgumentList.Add(cfg.ThresholdScale.ToString(CultureInfo.InvariantCulture));
        if (!cfg.AutoThreshold)
        {
            psi.ArgumentList.Add("--no-auto-threshold");
        }

        psi.ArgumentList.Add("--min-tone-purity");
        psi.ArgumentList.Add(cfg.MinTonePurity.ToString(CultureInfo.InvariantCulture));

        if (!cfg.ExperimentalRangeLock)
        {
            return;
        }

        psi.ArgumentList.Add("--experimental-range-lock");
        psi.ArgumentList.Add("--range-lock-min-hz");
        psi.ArgumentList.Add(cfg.RangeLockMinHz.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--range-lock-max-hz");
        psi.ArgumentList.Add(cfg.RangeLockMaxHz.ToString(CultureInfo.InvariantCulture));
    }

    private static async Task<string> RunOneShotAsync(ProcessStartInfo psi, CancellationToken ct)
    {
        using var process = Process.Start(psi)
            ?? throw new InvalidOperationException("Failed to start cw-decoder.");
        var stdoutTask = process.StandardOutput.ReadToEndAsync();
        var stderrTask = process.StandardError.ReadToEndAsync();
        await process.WaitForExitAsync(ct).ConfigureAwait(false);
        var stdout = await stdoutTask.ConfigureAwait(false);
        var stderr = await stderrTask.ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                string.IsNullOrWhiteSpace(stderr) ? $"cw-decoder exited with code {process.ExitCode}." : stderr.Trim());
        }
        return stdout;
    }

    private static string F(double value) => value.ToString(CultureInfo.InvariantCulture);

    private static bool TryParseHarvestProgress(
        string line,
        out int completed,
        out int total,
        out double startSeconds,
        out double endSeconds)
    {
        completed = 0;
        total = 0;
        startSeconds = 0;
        endSeconds = 0;

        var parts = line.Split('\t', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (parts.Length != 5 || !string.Equals(parts[0], "HARVEST_PROGRESS", StringComparison.Ordinal))
        {
            return false;
        }

        return int.TryParse(parts[1], CultureInfo.InvariantCulture, out completed)
            && int.TryParse(parts[2], CultureInfo.InvariantCulture, out total)
            && double.TryParse(parts[3], CultureInfo.InvariantCulture, out startSeconds)
            && double.TryParse(parts[4], CultureInfo.InvariantCulture, out endSeconds);
    }
}
