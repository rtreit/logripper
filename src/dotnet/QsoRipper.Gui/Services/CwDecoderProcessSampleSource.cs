using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Spawns the experimental <c>cw-decoder</c> Rust binary in
/// <c>stream-live-ditdah --json</c> mode and surfaces parsed <c>wpm</c>
/// NDJSON events as <see cref="CwWpmSample"/>s.
///
/// Round 1 deliberately reuses the experiment binary rather than the
/// not-yet-existent engine-side <c>CwDecodeService</c>. The binary is
/// located by walking up from the GUI's BaseDirectory, looking for the
/// experiment build output, in line with the existing experiment GUI's
/// discovery logic.
/// </summary>
internal sealed class CwDecoderProcessSampleSource : ICwWpmSampleSource
{
    public event EventHandler<CwWpmSample>? SampleReceived;
    public event EventHandler? StatusChanged;
    public event EventHandler<string>? RawLineReceived;
    public event EventHandler<CwLockState>? LockStateChanged;

    private Process? _proc;
    private CancellationTokenSource? _cts;
    private long _epoch;
    private CwWpmSample? _latest;
    private CwLockState _lockState = CwLockState.Unknown;
    private string? _lastStderrLine;
    private readonly object _stateLock = new();

    /// <summary>
    /// Last non-empty line written to stderr by the cw-decoder process. Used
    /// to surface the actual failure reason (e.g.
    /// "no output (loopback) device matching ...") when the process exits
    /// unexpectedly, instead of just showing "stopped".
    /// </summary>
    public string? LastStderrLine
    {
        get
        {
            lock (_stateLock)
            {
                return _lastStderrLine;
            }
        }
    }

    public bool IsRunning
    {
        get
        {
            lock (_stateLock)
            {
                return _proc is { HasExited: false };
            }
        }
    }

    public CwWpmSample? LatestSample
    {
        get
        {
            lock (_stateLock)
            {
                return _latest;
            }
        }
    }

    public CwLockState CurrentLockState
    {
        get
        {
            lock (_stateLock)
            {
                return _lockState;
            }
        }
    }

    public void Start(string? deviceOverride) => Start(deviceOverride, loopback: false, recordingPath: null);

    public void Start(string? deviceOverride, bool loopback) => Start(deviceOverride, loopback, recordingPath: null);

    /// <summary>
    /// Start the cw-decoder subprocess. When <paramref name="recordingPath"/>
    /// is supplied, also passes <c>--record &lt;path&gt;</c> so the decoder
    /// mirrors all captured audio into a WAV file alongside the live stream.
    /// The destination directory must already exist; the WAV file is created
    /// by the decoder itself when the input device is opened.
    /// </summary>
    public void Start(string? deviceOverride, bool loopback, string? recordingPath)
    {
        Stop();

        var exe = LocateBinary()
            ?? throw new InvalidOperationException(
                "Could not locate cw-decoder.exe. Build experiments/cw-decoder " +
                "(`cargo build --release` in src/rust or experiments/cw-decoder) " +
                "or set the CW_DECODER_EXE environment variable.");

        var psi = new ProcessStartInfo(exe)
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
            WorkingDirectory = Path.GetDirectoryName(exe)!,
        };
        psi.ArgumentList.Add("stream-live-ditdah");
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--window");
        psi.ArgumentList.Add("6");
        psi.ArgumentList.Add("--min-window");
        psi.ArgumentList.Add("4");
        psi.ArgumentList.Add("--decode-every-ms");
        psi.ArgumentList.Add("1000");
        psi.ArgumentList.Add("--confirmations");
        psi.ArgumentList.Add("1");
        if (loopback)
        {
            // WASAPI loopback: capture from a system OUTPUT device so audio
            // played to the speakers (e.g. a YouTube CW practice clip) is
            // decoded directly without going through speakers→room→mic.
            psi.ArgumentList.Add("--loopback");
        }
        if (!string.IsNullOrWhiteSpace(deviceOverride))
        {
            psi.ArgumentList.Add("--device");
            psi.ArgumentList.Add(deviceOverride.Trim());
        }
        if (!string.IsNullOrWhiteSpace(recordingPath))
        {
            psi.ArgumentList.Add("--record");
            psi.ArgumentList.Add(recordingPath.Trim());
        }

        var p = Process.Start(psi)
            ?? throw new InvalidOperationException("Failed to start cw-decoder.");

        var cts = new CancellationTokenSource();
        long epoch;
        lock (_stateLock)
        {
            _proc = p;
            _cts = cts;
            _latest = null;
            _lastStderrLine = null;
            _epoch = unchecked(_epoch + 1);
            epoch = _epoch;
        }

        StatusChanged?.Invoke(this, EventArgs.Empty);

        _ = Task.Run(() => PumpStdoutAsync(p, epoch, cts.Token));
        _ = Task.Run(() => PumpStderrAsync(p, epoch, cts.Token));
        _ = Task.Run(() =>
        {
            try
            { p.WaitForExit(); }
            catch (InvalidOperationException) { /* process disposed */ }
#pragma warning disable CA1031, RCS1075 // background watcher must not crash on shutdown
            catch (Exception)
            {
                // Best effort: WaitForExit can throw a variety of platform-specific
                // exceptions during process shutdown; we just want to fire the event.
            }
#pragma warning restore CA1031, RCS1075
            StatusChanged?.Invoke(this, EventArgs.Empty);
        });
    }

    public void Stop()
    {
        Process? proc;
        CancellationTokenSource? cts;
        lock (_stateLock)
        {
            proc = _proc;
            cts = _cts;
            _proc = null;
            _cts = null;
        }

        if (cts is not null)
        {
            try
            { cts.Cancel(); }
            catch (ObjectDisposedException) { /* ignore */ }
            cts.Dispose();
        }

        if (proc is { HasExited: false })
        {
            try
            {
                proc.StandardInput.WriteLine("stop");
                proc.StandardInput.Flush();
            }
            catch (IOException) { /* best effort */ }
            catch (InvalidOperationException) { /* best effort */ }
            try
            { proc.StandardInput.Close(); }
            catch (IOException) { /* best effort */ }
            if (!proc.WaitForExit(2000))
            {
                try
                { proc.Kill(entireProcessTree: true); }
                catch (InvalidOperationException) { /* best effort */ }
                catch (System.ComponentModel.Win32Exception) { /* best effort */ }
            }
        }

        proc?.Dispose();

        // Decoder went away; any cached lock state is meaningless to
        // GUI consumers now. Drop back to Unknown so the WPM/decoded
        // displays go fresh-stale-clear instead of remembering "Locked"
        // from the previous session.
        bool fireLockChange;
        lock (_stateLock)
        {
            fireLockChange = _lockState != CwLockState.Unknown;
            _lockState = CwLockState.Unknown;
        }
        if (fireLockChange)
        {
            LockStateChanged?.Invoke(this, CwLockState.Unknown);
        }

        StatusChanged?.Invoke(this, EventArgs.Empty);
    }

    public void Dispose() => Stop();

    /// <summary>
    /// Send a "reset_lock" command to the running decoder. The decoder
    /// drops its current pitch lock and resumes hunting so the next
    /// QSO does not inherit the previous station's tone/timing state.
    /// No-op if the decoder is not running. Best-effort: pipe write
    /// failures during shutdown are swallowed.
    /// </summary>
    public void ResetLock()
    {
        Process? proc;
        lock (_stateLock)
        {
            proc = _proc;
        }
        if (proc is null || proc.HasExited)
        {
            return;
        }
        try
        {
            proc.StandardInput.WriteLine("{\"type\":\"reset_lock\"}");
            proc.StandardInput.Flush();
        }
        catch (IOException) { /* best effort */ }
        catch (ObjectDisposedException) { /* best effort */ }
        catch (InvalidOperationException) { /* best effort */ }
    }

    private async Task PumpStdoutAsync(Process p, long epoch, CancellationToken ct)
    {
        try
        {
            string? line;
            while (!ct.IsCancellationRequested
                   && (line = await p.StandardOutput.ReadLineAsync(ct).ConfigureAwait(false)) is not null)
            {
                if (string.IsNullOrWhiteSpace(line))
                {
                    continue;
                }

                // Tee the full raw line to subscribers (diagnostics recorder)
                // BEFORE WPM-specific parsing so they see every event the
                // decoder emits — confidence, pitch, char, garbled, power, …
                RawLineReceived?.Invoke(this, line);

                if (TryParseConfidenceEvent(line, out var newState))
                {
                    bool changed;
                    lock (_stateLock)
                    {
                        changed = _lockState != newState;
                        _lockState = newState;
                    }
                    if (changed)
                    {
                        LockStateChanged?.Invoke(this, newState);
                    }
                }
                else if (TryParsePitchLostEvent(line))
                {
                    // pitch_lost is the decoder's own "lock dropped"
                    // signal; the next confidence event will follow but
                    // we surface the lock loss immediately so the GUI
                    // doesn't keep showing a stale WPM for the gap.
                    bool changed;
                    lock (_stateLock)
                    {
                        changed = _lockState != CwLockState.Hunting;
                        _lockState = CwLockState.Hunting;
                    }
                    if (changed)
                    {
                        LockStateChanged?.Invoke(this, CwLockState.Hunting);
                    }
                }

                if (TryParseWpmEvent(line, out var wpm))
                {
                    var sample = new CwWpmSample(DateTimeOffset.UtcNow, wpm, epoch);
                    lock (_stateLock)
                    {
                        _latest = sample;
                    }
                    SampleReceived?.Invoke(this, sample);
                }
            }
        }
        catch (OperationCanceledException)
        {
            // Expected on Stop().
        }
        catch (IOException)
        {
            // Stdout pipe closed during shutdown.
        }
    }

    private async Task PumpStderrAsync(Process p, long epoch, CancellationToken ct)
    {
        try
        {
            string? line;
            while (!ct.IsCancellationRequested
                   && (line = await p.StandardError.ReadLineAsync(ct).ConfigureAwait(false)) is not null)
            {
                if (string.IsNullOrWhiteSpace(line))
                {
                    continue;
                }

                lock (_stateLock)
                {
                    // Only retain stderr from the *current* epoch; ignore late
                    // lines from a stopped predecessor.
                    if (_epoch == epoch)
                    {
                        _lastStderrLine = line.Trim();
                    }
                }
            }
        }
        catch (OperationCanceledException)
        {
            // Expected on Stop().
        }
        catch (IOException)
        {
            // Stderr pipe closed during shutdown.
        }
    }

    private static bool TryParseConfidenceEvent(string ndjsonLine, out CwLockState state)
    {
        state = CwLockState.Unknown;
        try
        {
            using var doc = JsonDocument.Parse(ndjsonLine);
            var root = doc.RootElement;
            if (!root.TryGetProperty("type", out var typeProp)
                || typeProp.ValueKind != JsonValueKind.String
                || !string.Equals(typeProp.GetString(), "confidence", StringComparison.Ordinal))
            {
                return false;
            }
            if (!root.TryGetProperty("state", out var stateProp)
                || stateProp.ValueKind != JsonValueKind.String)
            {
                return false;
            }
            state = stateProp.GetString() switch
            {
                "locked" => CwLockState.Locked,
                "probation" => CwLockState.Probation,
                "hunting" => CwLockState.Hunting,
                _ => CwLockState.Unknown,
            };
            return true;
        }
        catch (JsonException)
        {
            return false;
        }
    }

    private static bool TryParsePitchLostEvent(string ndjsonLine)
    {
        try
        {
            using var doc = JsonDocument.Parse(ndjsonLine);
            return doc.RootElement.TryGetProperty("type", out var typeProp)
                && typeProp.ValueKind == JsonValueKind.String
                && string.Equals(typeProp.GetString(), "pitch_lost", StringComparison.Ordinal);
        }
        catch (JsonException)
        {
            return false;
        }
    }

    private static bool TryParseWpmEvent(string ndjsonLine, out double wpm)
    {
        wpm = 0;
        try
        {
            using var doc = JsonDocument.Parse(ndjsonLine);
            var root = doc.RootElement;
            if (!root.TryGetProperty("type", out var typeProp)
                || typeProp.ValueKind != JsonValueKind.String
                || !string.Equals(typeProp.GetString(), "wpm", StringComparison.Ordinal))
            {
                return false;
            }

            if (!root.TryGetProperty("wpm", out var wpmProp))
            {
                return false;
            }

            if (wpmProp.ValueKind == JsonValueKind.Number && wpmProp.TryGetDouble(out var value)
                && double.IsFinite(value) && value > 0)
            {
                wpm = value;
                return true;
            }
        }
        catch (JsonException)
        {
            // Non-JSON or malformed line — ignore quietly; the experiment
            // binary occasionally emits stray non-NDJSON status lines.
        }

        return false;
    }

    /// <summary>
    /// Locates the experiment <c>cw-decoder</c> binary. Mirrors the
    /// experiment GUI's discovery logic so the same build artifacts work
    /// for both surfaces.
    /// </summary>
    internal static string? LocateBinary()
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
            var candidates = new[]
            {
                Path.Combine(dir.FullName, exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "release", exeName),
                Path.Combine(dir.FullName, "experiments", "cw-decoder", "target", "debug", exeName),
                Path.Combine(dir.FullName, "target", "release", exeName),
                Path.Combine(dir.FullName, "target", "debug", exeName),
            };

            var newest = candidates
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
}
