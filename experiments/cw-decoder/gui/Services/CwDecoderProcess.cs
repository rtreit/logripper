using System;
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
        var exe = LocateBinary();
        if (exe is null) return Array.Empty<string>();
        try
        {
            var psi = new ProcessStartInfo(exe, "devices")
            {
                RedirectStandardOutput = true,
                UseShellExecute = false,
                CreateNoWindow = true,
            };
            using var p = Process.Start(psi);
            if (p is null) return Array.Empty<string>();
            var lines = p.StandardOutput.ReadToEnd().Split('\n', StringSplitOptions.RemoveEmptyEntries);
            p.WaitForExit(3000);
            var result = new System.Collections.Generic.List<string>();
            foreach (var raw in lines)
            {
                var line = raw.TrimEnd('\r').TrimStart();
                if (line.StartsWith("- ")) result.Add(line[2..].Trim());
            }
            return result.ToArray();
        }
        catch { return Array.Empty<string>(); }
    }

    public void StartLive(string? device, DecoderConfig cfg, BaselineDecoderConfig baselineCfg, bool useBaseline, string? recordPath = null)
    {
        Stop();
        // For live capture, balance latency vs accuracy: cadence stays fast
        // (400ms) but require 2 confirmations on a stable rolling window so
        // we don't commit garbage from partial sub-second windows. With
        // confirmations=1 + tiny min-window, the prefix-stabilizer was
        // appending every wildly-different rolling-decode to the transcript,
        // producing massive duplicated output (CER >2000%).
        var liveBaselineArgs = "--window 6 --min-window 1.5 --decode-every-ms 400 --confirmations 2";
        var args = useBaseline
            ? $"stream-live-ditdah --json --chunk-ms 50 {liveBaselineArgs}"
            : $"stream-live --json --stdin-control {cfg.ToCliArgs()}";
        if (!string.IsNullOrWhiteSpace(device)) args += $" --device \"{device}\"";
        if (!string.IsNullOrWhiteSpace(recordPath)) args += $" --record \"{recordPath}\"";
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
        double pitchHz,
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
        psi.ArgumentList.Add("--pitch-hz");
        psi.ArgumentList.Add(F(pitchHz));
        if (wpm is double wpmValue && wpmValue > 0)
        {
            psi.ArgumentList.Add("--wpm");
            psi.ArgumentList.Add(F(wpmValue));
        }

        var stdout = await RunOneShotAsync(psi, ct).ConfigureAwait(false);
        return JsonSerializer.Deserialize<SignalProfile>(stdout)
            ?? throw new InvalidOperationException("Failed to parse profile-window output.");
    }

    public async Task<string> RunLabelScoreAsync(
        bool allLabels,
        string? labelPath,
        bool fullStreamMode,
        int preRollMs,
        int postRollMs,
        double windowSeconds,
        double minWindowSeconds,
        int decodeEveryMs,
        int confirmations,
        CancellationToken ct = default)
    {
        var psi = CreateEvalStartInfo();
        AddLabelSelectionArguments(psi, allLabels, labelPath);
        AddLabelScoreModeArguments(psi, fullStreamMode, preRollMs, postRollMs);
        psi.ArgumentList.Add("--window");
        psi.ArgumentList.Add(F(windowSeconds));
        psi.ArgumentList.Add("--min-window");
        psi.ArgumentList.Add(F(minWindowSeconds));
        psi.ArgumentList.Add("--decode-every-ms");
        psi.ArgumentList.Add(decodeEveryMs.ToString(CultureInfo.InvariantCulture));
        psi.ArgumentList.Add("--confirmations");
        psi.ArgumentList.Add(confirmations.ToString(CultureInfo.InvariantCulture));
        return await RunOneShotAsync(psi, ct).ConfigureAwait(false);
    }

    public async Task<string> RunLabelSweepAsync(
        bool allLabels,
        string? labelPath,
        bool fullStreamMode,
        int preRollMs,
        int postRollMs,
        bool wideSweep,
        int top,
        CancellationToken ct = default)
    {
        var psi = CreateEvalStartInfo();
        AddLabelSelectionArguments(psi, allLabels, labelPath);
        AddLabelScoreModeArguments(psi, fullStreamMode, preRollMs, postRollMs);
        psi.ArgumentList.Add("--sweep-ditdah");
        psi.ArgumentList.Add("--top");
        psi.ArgumentList.Add(top.ToString(CultureInfo.InvariantCulture));
        if (wideSweep)
        {
            psi.ArgumentList.Add("--wide-sweep");
        }

        return await RunOneShotAsync(psi, ct).ConfigureAwait(false);
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
                // Close stdin first so the Rust child sees EOF and exits its
                // main loop normally — that lets Drop run on LiveCapture and
                // hound finalize the WAV header. Without this, Kill leaves
                // the recording with a header-only / "missing data chunk"
                // file that Replay & Score can't read.
                try { proc.StandardInput.Close(); } catch { /* best effort */ }
                if (!proc.WaitForExit(1500))
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

    private static void AddLabelSelectionArguments(ProcessStartInfo psi, bool allLabels, string? labelPath)
    {
        if (allLabels)
        {
            psi.ArgumentList.Add("--labels-dir");
            psi.ArgumentList.Add(LocateLabelCorpusDirectory());
            return;
        }

        if (string.IsNullOrWhiteSpace(labelPath))
        {
            throw new InvalidOperationException("Pick an audio file with saved labels first, or enable all-labels.");
        }

        psi.ArgumentList.Add("--labels");
        psi.ArgumentList.Add(labelPath);
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
