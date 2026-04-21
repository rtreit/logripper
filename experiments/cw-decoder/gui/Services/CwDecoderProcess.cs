using System;
using System.Diagnostics;
using System.IO;
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

    public void StartLive(string? device)
    {
        Stop();
        var args = "stream-live --json";
        if (!string.IsNullOrWhiteSpace(device)) args += $" --device \"{device}\"";
        Spawn(args);
    }

    public void StartFile(string path, bool realtime)
    {
        Stop();
        var args = $"stream-file --json \"{path}\"";
        if (realtime) args += " --realtime";
        Spawn(args);
    }

    public void Stop()
    {
        try
        {
            _cts?.Cancel();
            if (_proc is { HasExited: false })
            {
                try { _proc.Kill(entireProcessTree: true); } catch { /* best effort */ }
            }
        }
        catch { /* ignored */ }
        _proc = null;
        _cts = null;
    }

    public void Dispose() => Stop();

    private void Spawn(string args)
    {
        var exe = LocateBinary() ?? throw new InvalidOperationException(
            "Could not locate cw-decoder.exe. Run `cargo build --release` in experiments/cw-decoder first.");
        var psi = new ProcessStartInfo(exe, args)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(exe)!,
        };
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
            // Inside the cw-decoder folder?
            var rel1 = Path.Combine(dir.FullName, "target", "release", exeName);
            if (File.Exists(rel1)) return rel1;
            var rel2 = Path.Combine(dir.FullName, "target", "debug", exeName);
            if (File.Exists(rel2)) return rel2;
            // Or in the parent?
            var rel3 = Path.Combine(dir.FullName, "cw-decoder", "target", "release", exeName);
            if (File.Exists(rel3)) return rel3;
            var rel4 = Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "release", exeName);
            if (File.Exists(rel4)) return rel4;
        }
        return null;
    }
}
