using System;
using System.Collections.Generic;
using System.Linq;
using System.Runtime.CompilerServices;
using Avalonia.Threading;
using CwDecoderGui.Models;
using CwDecoderGui.Services;

namespace CwDecoderGui.ViewModels;

/// <summary>
/// Immutable snapshot of the live envelope decoder state, produced from
/// one stream-live-v3 'viz' NDJSON event. The Visualizer canvas binds to
/// this; swapping the property to a fresh instance triggers AffectsRender.
/// </summary>
public sealed class VizFrameVm
{
    public double[] Envelope { get; }
    public double EnvelopeMax { get; }
    public double NoiseFloor { get; }
    public double SignalFloor { get; }
    public double SnrDb { get; }
    public bool SnrSuppressed { get; }
    public double HystHigh { get; }
    public double HystLow { get; }
    public double BufferSeconds { get; }
    public double FrameStepS { get; }
    public double DotSeconds { get; }
    public double Wpm { get; }
    public double WpmKmeans { get; }
    public double? LockedWpm { get; }
    public double CentroidDot { get; }
    public double CentroidDah { get; }
    public double PitchHz { get; }
    public double[] OnDurations { get; }
    public IReadOnlyList<VizEventVm> Events { get; }

    internal VizFrameVm(DecoderEvent ev)
    {
        Envelope = ev.Envelope ?? Array.Empty<double>();
        EnvelopeMax = ev.EnvelopeMax ?? 0;
        NoiseFloor = ev.NoiseFloor ?? 0;
        SignalFloor = ev.SignalFloor ?? 0;
        SnrDb = ev.SnrDb ?? 0;
        SnrSuppressed = ev.SnrSuppressed ?? false;
        HystHigh = ev.HystHigh ?? 0;
        HystLow = ev.HystLow ?? 0;
        BufferSeconds = ev.BufferSeconds ?? 0;
        FrameStepS = ev.FrameStepS ?? 0;
        DotSeconds = ev.DotSeconds ?? 0;
        Wpm = ev.Wpm ?? 0;
        WpmKmeans = ev.WpmKmeans ?? (ev.Wpm ?? 0);
        LockedWpm = ev.LockedWpm;
        CentroidDot = ev.CentroidDot ?? 0;
        CentroidDah = ev.CentroidDah ?? 0;
        PitchHz = ev.PitchHz ?? 0;
        OnDurations = ev.OnDurations ?? Array.Empty<double>();
        Events = ev.Events?.Select(e => new VizEventVm(e.StartS, e.EndS, e.DurationS, e.Kind)).ToList()
                 ?? new List<VizEventVm>();
    }

    private VizFrameVm()
    {
        Envelope = Array.Empty<double>();
        OnDurations = Array.Empty<double>();
        Events = Array.Empty<VizEventVm>();
    }

    public static VizFrameVm Empty { get; } = new VizFrameVm();
}

public sealed record VizEventVm(double StartS, double EndS, double DurationS, string Kind);

public sealed partial class MainWindowViewModel
{
    private readonly CwDecoderProcess _vizProcess = new();
    private readonly AudioPlaybackProcess _vizPlayback = new();
    private bool _vizWired;

    private VizFrameVm _vizFrame = VizFrameVm.Empty;
    public VizFrameVm VizFrame { get => _vizFrame; set => Set(ref _vizFrame, value); }

    private string _vizTranscript = "";
    public string VizTranscript { get => _vizTranscript; set => Set(ref _vizTranscript, value); }

    private double _vizWindowSeconds = 10.0;
    public double VizWindowSeconds { get => _vizWindowSeconds; set => Set(ref _vizWindowSeconds, value); }

    private double _vizCurrentWpm;
    public double VizCurrentWpm { get => _vizCurrentWpm; set => Set(ref _vizCurrentWpm, value); }

    private bool _vizRunning;
    public bool VizRunning
    {
        get => _vizRunning;
        set
        {
            if (Set(ref _vizRunning, value))
            {
                OnPropertyChanged(nameof(VizStartStopLabel));
            }
        }
    }

    private string _vizStatus = "idle";
    public string VizStatus { get => _vizStatus; set => Set(ref _vizStatus, value); }

    private bool _vizUseLoopback;
    public bool VizUseLoopback { get => _vizUseLoopback; set => Set(ref _vizUseLoopback, value); }

    private double _vizPinWpm;
    public double VizPinWpm { get => _vizPinWpm; set => Set(ref _vizPinWpm, value); }

    private double _vizPinHz;
    public double VizPinHz { get => _vizPinHz; set => Set(ref _vizPinHz, value); }

    private bool _vizMute;
    /// <summary>
    /// When true, PLAY FILE on the visualizer tab still drives the decoder
    /// pipeline but does not stream the WAV audio to the default output
    /// device. Useful for screen capture or unattended runs. Defaults to
    /// false so the operator can hear the file and watch the visualizer
    /// react to it together (without this they get a silent visualizer,
    /// which earlier looked like a missing-feature bug).
    /// </summary>
    public bool VizMute { get => _vizMute; set => Set(ref _vizMute, value); }

    private bool _vizUsePeriodWpm = true;
    /// <summary>
    /// When true (default) the visualizer's "WPM" readout uses the
    /// period-based dot estimator from the decoder (rising-edge intervals,
    /// invariant to compander/threshold bias). When false it falls back
    /// to the legacy k-means dot WPM emitted as <c>wpm_kmeans</c>.
    /// Toggling does NOT restart the decoder; both values are emitted on
    /// every viz frame so the swap is instant.
    /// </summary>
    public bool VizUsePeriodWpm
    {
        get => _vizUsePeriodWpm;
        set
        {
            if (Set(ref _vizUsePeriodWpm, value))
            {
                // Refresh the displayed WPM using the most recent frame
                // so the change is visible immediately.
                if (_lastVizWpmPeriod.HasValue || _lastVizWpmKmeans.HasValue)
                {
                    VizCurrentWpm = value
                        ? (_lastVizWpmPeriod ?? _lastVizWpmKmeans ?? 0)
                        : (_lastVizWpmKmeans ?? _lastVizWpmPeriod ?? 0);
                }
            }
        }
    }

    private double? _lastVizWpmPeriod;
    private double? _lastVizWpmKmeans;

    public string VizStartStopLabel => VizRunning ? "STOP" : "START LIVE";

    /// <summary>Resolves the persistent capture directory and ensures it exists.</summary>
    private static string ResolveVizCaptureDir()
    {
        // Walk up from the running exe to find the experiments\cw-decoder root.
        var dir = AppContext.BaseDirectory;
        for (int i = 0; i < 8 && !string.IsNullOrEmpty(dir); i++)
        {
            var candidate = System.IO.Path.Combine(dir, "captures");
            var marker = System.IO.Path.Combine(dir, "Cargo.toml");
            if (System.IO.File.Exists(marker))
            {
                System.IO.Directory.CreateDirectory(candidate);
                return candidate;
            }
            dir = System.IO.Path.GetDirectoryName(dir) ?? "";
        }
        // Fallback: cwd\captures.
        var fallback = System.IO.Path.Combine(Environment.CurrentDirectory, "captures");
        System.IO.Directory.CreateDirectory(fallback);
        return fallback;
    }

    /// <summary>Toggle the live envelope visualizer.</summary>
    public void ToggleViz()
    {
        if (VizRunning) StopViz();
        else StartViz();
    }

    public void StartViz()
    {
        EnsureVizWired();
        try
        {
            VizTranscript = "";
            VizFrame = VizFrameVm.Empty;
            VizCurrentWpm = 0;
            VizStatus = "starting…";
            VizBarMonitor.Reset("live");
            // Auto-save every live capture so it can be labeled later.
            var stamp = DateTime.Now.ToString("yyyyMMdd-HHmmss-fff");
            var captureDir = ResolveVizCaptureDir();
            var recordPath = System.IO.Path.Combine(captureDir, $"viz-{stamp}.wav");
            _vizProcess.StartLiveV3(SelectedDevice, decodeEveryMs: 250,
                recordPath: recordPath, loopback: VizUseLoopback,
                pinWpm: VizPinWpm, pinHz: VizPinHz);
            VizRunning = true;
            VizStatus = (VizUseLoopback ? "live (loopback)" : "live (mic)") +
                $" → captures\\viz-{stamp}.wav";
        }
        catch (Exception ex)
        {
            VizStatus = $"error: {ex.Message}";
            VizRunning = false;
        }
    }

    public void StopViz()
    {
        try { _vizProcess.Stop(); } catch { /* best effort */ }
        try { _vizPlayback.Stop(); } catch { /* best effort */ }
        VizRunning = false;
        VizStatus = "stopped";
        var flushed = VizBarMonitor.Flush();
        if (flushed is not null) VizStatus = $"stopped → {System.IO.Path.GetFileName(flushed)}";
    }

    public void StartVizFile(string filePath)
    {
        EnsureVizWired();
        try
        {
            VizTranscript = "";
            VizFrame = VizFrameVm.Empty;
            VizCurrentWpm = 0;
            VizStatus = $"file: {System.IO.Path.GetFileName(filePath)}";
            VizBarMonitor.Reset(System.IO.Path.GetFileNameWithoutExtension(filePath));
            _vizProcess.StartFileV3(filePath, decodeEveryMs: 250,
                pinWpm: VizPinWpm, pinHz: VizPinHz, playAudio: !VizMute);
            try { _vizPlayback.Stop(); } catch { /* best effort */ }

            VizRunning = true;
        }
        catch (Exception ex)
        {
            VizStatus = $"error: {ex.Message}";
            VizRunning = false;
        }
    }

    private void EnsureVizWired()
    {
        if (_vizWired) return;
        _vizWired = true;
        _vizProcess.EventReceived += OnVizEvent;
        _vizProcess.Exited += _ => Dispatcher.UIThread.Post(() =>
        {
            VizRunning = false;
            try { _vizPlayback.Stop(); } catch { /* best effort */ }
            var flushed = VizBarMonitor.Flush();
            if (VizStatus.StartsWith("live", StringComparison.OrdinalIgnoreCase))
            {
                VizStatus = "process exited";
            }
            if (flushed is not null) VizStatus += $" → {System.IO.Path.GetFileName(flushed)}";
        });
    }

    private void OnVizEvent(DecoderEvent ev)
    {
        Dispatcher.UIThread.Post(() =>
        {
            switch (ev.Type)
            {
                case "ready":
                    VizStatus = $"ready @ {ev.Rate ?? 0} Hz";
                    break;
                case "transcript":
                    // PR #370 (Approach A+): the Rust side now emits a
                    // sample-indexed cumulative transcript via
                    // LiveCommitCursor (`transcript` = committed +
                    // provisional). It is monotonic and idempotent.
                    //
                    // Do NOT fall back to `ev.Text` (the rolling-window
                    // re-decode). That field is produced by a different
                    // decode path and can disagree with the cursor's
                    // text; switching between the two between the
                    // pre-lock and post-lock cycles makes the
                    // transcript appear to vanish and get replaced when
                    // the cursor first commits. Show the cursor text
                    // only — empty until the streamer locks is
                    // expected and accurate.
                    var sess = ev.Transcript;
                    if (sess is not null) VizTranscript = sess;
                    if (ev.Wpm.HasValue) _lastVizWpmPeriod = ev.Wpm.Value;
                    if (ev.WpmKmeans.HasValue) _lastVizWpmKmeans = ev.WpmKmeans.Value;
                    if (ev.Wpm.HasValue || ev.WpmKmeans.HasValue)
                    {
                        VizCurrentWpm = VizUsePeriodWpm
                            ? (_lastVizWpmPeriod ?? _lastVizWpmKmeans ?? 0)
                            : (_lastVizWpmKmeans ?? _lastVizWpmPeriod ?? 0);
                    }
                    break;
                case "viz":
                    VizFrame = new VizFrameVm(ev);
                    VizBarMonitor.Ingest(ev);
                    if (ev.Wpm.HasValue) _lastVizWpmPeriod = ev.Wpm.Value;
                    if (ev.WpmKmeans.HasValue) _lastVizWpmKmeans = ev.WpmKmeans.Value;
                    if (ev.Wpm.HasValue || ev.WpmKmeans.HasValue)
                    {
                        VizCurrentWpm = VizUsePeriodWpm
                            ? (_lastVizWpmPeriod ?? _lastVizWpmKmeans ?? 0)
                            : (_lastVizWpmKmeans ?? _lastVizWpmPeriod ?? 0);
                    }
                    break;
                case "end":
                    if (ev.Transcript is not null) VizTranscript = ev.Transcript;
                    VizRunning = false;
                    VizStatus = "ended";
                    var flushed = VizBarMonitor.Flush();
                    if (flushed is not null) VizStatus = $"ended → {System.IO.Path.GetFileName(flushed)}";
                    break;
            }
        });
    }
}
