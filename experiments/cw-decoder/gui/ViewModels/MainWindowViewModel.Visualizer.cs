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
    public double HystHigh { get; }
    public double HystLow { get; }
    public double BufferSeconds { get; }
    public double FrameStepS { get; }
    public double DotSeconds { get; }
    public double Wpm { get; }
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
        HystHigh = ev.HystHigh ?? 0;
        HystLow = ev.HystLow ?? 0;
        BufferSeconds = ev.BufferSeconds ?? 0;
        FrameStepS = ev.FrameStepS ?? 0;
        DotSeconds = ev.DotSeconds ?? 0;
        Wpm = ev.Wpm ?? 0;
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

    public string VizStartStopLabel => VizRunning ? "STOP" : "START LIVE";

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
            _vizProcess.StartLiveV3(SelectedDevice, decodeEveryMs: 250,
                recordPath: null, loopback: VizUseLoopback, pinWpm: VizPinWpm);
            VizRunning = true;
            VizStatus = VizUseLoopback ? "live (loopback)" : "live (mic)";
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
        VizRunning = false;
        VizStatus = "stopped";
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
            _vizProcess.StartFileV3(filePath, decodeEveryMs: 250, pinWpm: VizPinWpm);
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
            if (VizStatus.StartsWith("live", StringComparison.OrdinalIgnoreCase))
            {
                VizStatus = "process exited";
            }
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
                    if (ev.Text is not null) VizTranscript = ev.Text;
                    if (ev.Wpm.HasValue) VizCurrentWpm = ev.Wpm.Value;
                    break;
                case "viz":
                    VizFrame = new VizFrameVm(ev);
                    if (ev.Wpm.HasValue) VizCurrentWpm = ev.Wpm.Value;
                    break;
                case "end":
                    if (ev.Transcript is not null) VizTranscript = ev.Transcript;
                    VizRunning = false;
                    VizStatus = "ended";
                    break;
            }
        });
    }
}
