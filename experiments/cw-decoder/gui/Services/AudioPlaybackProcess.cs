using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using CwDecoderGui.Models;

namespace CwDecoderGui.Services;

internal sealed class AudioPlaybackProcess : IDisposable
{
    private Process? _proc;
    private CancellationTokenSource? _cts;

    public event Action<PlaybackEvent>? EventReceived;
    public event Action<string>? StderrLine;
    public event Action<int>? Exited;

    public void Start(string path)
    {
        Stop();

        var psi = CreateBaseStartInfo();
        psi.ArgumentList.Add("play-file");
        psi.ArgumentList.Add(path);
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--stdin-control");
        psi.RedirectStandardInput = true;

        var process = Process.Start(psi) ?? throw new InvalidOperationException("Failed to start cw-decoder playback.");
        _proc = process;
        _cts = new CancellationTokenSource();
        _ = Task.Run(() => PumpStdoutAsync(process, _cts.Token));
        _ = Task.Run(() => PumpStderrAsync(process, _cts.Token));
        _ = Task.Run(() =>
        {
            try { process.WaitForExit(); } catch { }
            Exited?.Invoke(process.ExitCode);
        });
    }

    /// <summary>Pause the running play-file process. No-op otherwise.</summary>
    public void Pause() => SendCommand("{\"cmd\":\"pause\"}");

    /// <summary>Resume the running play-file process. No-op otherwise.</summary>
    public void Resume() => SendCommand("{\"cmd\":\"resume\"}");

    /// <summary>
    /// Seek the running play-file process to <paramref name="positionSeconds"/>
    /// (in original-file seconds). No-op when not playing.
    /// </summary>
    public void Seek(double positionSeconds)
    {
        var ic = System.Globalization.CultureInfo.InvariantCulture;
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

    public void Stop()
    {
        try
        {
            _cts?.Cancel();
            if (_proc is { HasExited: false } proc)
            {
                try { proc.Kill(entireProcessTree: true); } catch { }
            }
        }
        catch
        {
        }

        _proc = null;
        _cts = null;
    }

    public void Dispose() => Stop();

    private async Task PumpStdoutAsync(Process process, CancellationToken ct)
    {
        try
        {
            string? line;
            while (!ct.IsCancellationRequested
                && (line = await process.StandardOutput.ReadLineAsync().ConfigureAwait(false)) is not null)
            {
                if (string.IsNullOrWhiteSpace(line))
                {
                    continue;
                }

                PlaybackEvent? ev = null;
                try { ev = JsonSerializer.Deserialize<PlaybackEvent>(line); }
                catch (JsonException) { }

                if (ev is not null)
                {
                    EventReceived?.Invoke(ev);
                }
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            StderrLine?.Invoke($"[gui] playback stdout error: {ex.Message}");
        }
    }

    private async Task PumpStderrAsync(Process process, CancellationToken ct)
    {
        try
        {
            string? line;
            while (!ct.IsCancellationRequested
                && (line = await process.StandardError.ReadLineAsync().ConfigureAwait(false)) is not null)
            {
                StderrLine?.Invoke(line);
            }
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            StderrLine?.Invoke($"[gui] playback stderr error: {ex.Message}");
        }
    }

    private static string? LocateBinary()
    {
        var env = Environment.GetEnvironmentVariable("CW_DECODER_EXE");
        if (!string.IsNullOrWhiteSpace(env) && File.Exists(env))
        {
            return env;
        }

        var exeName = OperatingSystem.IsWindows() ? "cw-decoder.exe" : "cw-decoder";
        var dir = new DirectoryInfo(AppContext.BaseDirectory);
        for (int i = 0; dir is not null && i < 8; i++, dir = dir.Parent)
        {
            var newest = new[]
            {
                Path.Combine(dir.FullName, "target", "release", exeName),
                Path.Combine(dir.FullName, "target", "debug", exeName),
                Path.Combine(dir.FullName, "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "cw-decoder", "target", "debug", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "debug", exeName),
            }
            .Where(File.Exists)
            .Select(path => new FileInfo(path))
            .OrderByDescending(info => info.LastWriteTimeUtc)
            .FirstOrDefault();

            if (newest is not null)
            {
                return newest.FullName;
            }
        }

        return null;
    }

    private static ProcessStartInfo CreateBaseStartInfo()
    {
        var exe = LocateBinary() ?? throw new InvalidOperationException(
            "Could not locate cw-decoder.exe. Run `cargo build --release` in experiments/cw-decoder first.");

        return new ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(exe)!,
        };
    }
}
