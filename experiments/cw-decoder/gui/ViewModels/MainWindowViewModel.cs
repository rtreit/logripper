using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using Avalonia.Threading;
using CwDecoderGui.Models;
using CwDecoderGui.Services;

namespace CwDecoderGui.ViewModels;

public sealed class MainWindowViewModel : INotifyPropertyChanged, IDisposable
{
    private readonly CwDecoderProcess _process = new();

    public MainWindowViewModel()
    {
        Devices = new ObservableCollection<string>(CwDecoderProcess.ListDevices());
        SelectedDevice = Devices.Count > 0 ? Devices[0] : null;
        Cells = new ObservableCollection<TranscriptCell>();
        WpmHistory = new ObservableCollection<double>();

        _process.EventReceived += OnEvent;
        _process.StderrLine += line => Dispatcher.UIThread.Post(() => StatusText = line);
        _process.Exited += code => Dispatcher.UIThread.Post(() =>
        {
            IsRunning = false;
            StatusText = code == 0 ? "Stopped." : $"Decoder exited (code {code}).";
        });
    }

    public ObservableCollection<string> Devices { get; }
    public ObservableCollection<TranscriptCell> Cells { get; }
    public ObservableCollection<double> WpmHistory { get; }

    private string? _selectedDevice;
    public string? SelectedDevice { get => _selectedDevice; set => Set(ref _selectedDevice, value); }

    private bool _isRunning;
    public bool IsRunning
    {
        get => _isRunning;
        set
        {
            if (Set(ref _isRunning, value))
                OnPropertyChanged(nameof(StartStopLabel));
        }
    }

    public string StartStopLabel => IsRunning ? "STOP" : "START";

    private bool _hideDecoded;
    public bool HideDecoded { get => _hideDecoded; set => Set(ref _hideDecoded, value); }

    private double _wpm;
    public double Wpm { get => _wpm; set => Set(ref _wpm, value); }

    private double _pitchHz;
    public double PitchHz { get => _pitchHz; set => Set(ref _pitchHz, value); }

    private double _power;
    public double Power { get => _power; set => Set(ref _power, value); }

    private double _threshold;
    public double Threshold { get => _threshold; set => Set(ref _threshold, value); }

    private bool _signal;
    public bool Signal { get => _signal; set => Set(ref _signal, value); }

    private string? _statusText;
    public string? StatusText { get => _statusText; set => Set(ref _statusText, value); }

    private string _sourceLabel = "(idle)";
    public string SourceLabel { get => _sourceLabel; set => Set(ref _sourceLabel, value); }

    private double _normalizedLevel;
    /// <summary>0.0 .. 1.0, calibrated against rolling max power for the meter bar.</summary>
    public double NormalizedLevel { get => _normalizedLevel; set => Set(ref _normalizedLevel, value); }

    private double _normalizedThreshold;
    public double NormalizedThreshold { get => _normalizedThreshold; set => Set(ref _normalizedThreshold, value); }

    private const int MaxWpmHistory = 200;
    private double _powerCeiling = 1e-6;

    public void ToggleStartStop()
    {
        if (IsRunning) { _process.Stop(); IsRunning = false; return; }
        Cells.Clear();
        WpmHistory.Clear();
        Wpm = 0;
        PitchHz = 0;
        Power = 0;
        Threshold = 0;
        NormalizedLevel = 0;
        NormalizedThreshold = 0;
        _powerCeiling = 1e-6;
        StatusText = "Starting…";
        _process.StartLive(SelectedDevice);
        IsRunning = true;
    }

    public void OpenFile(string path)
    {
        if (IsRunning) { _process.Stop(); IsRunning = false; }
        Cells.Clear();
        WpmHistory.Clear();
        Wpm = 0;
        _powerCeiling = 1e-6;
        StatusText = $"Decoding {path}";
        _process.StartFile(path, realtime: true);
        IsRunning = true;
    }

    public void RefreshDevices()
    {
        var fresh = CwDecoderProcess.ListDevices();
        Devices.Clear();
        foreach (var d in fresh) Devices.Add(d);
        if (SelectedDevice is null && Devices.Count > 0) SelectedDevice = Devices[0];
    }

    private void OnEvent(DecoderEvent ev)
    {
        Dispatcher.UIThread.Post(() => Apply(ev));
    }

    private void Apply(DecoderEvent ev)
    {
        switch (ev.Type)
        {
            case "ready":
                SourceLabel = ev.Source == "live"
                    ? $"LIVE · {ev.Device} · {ev.Rate} Hz"
                    : $"FILE · {System.IO.Path.GetFileName(ev.Path ?? "?")}";
                StatusText = "Listening for pitch lock…";
                break;
            case "pitch":
                if (ev.Hz is double hz)
                {
                    PitchHz = hz;
                    StatusText = $"Pitch lock: {hz:F1} Hz";
                }
                break;
            case "wpm":
                if (ev.Wpm is double wpm)
                {
                    Wpm = wpm;
                    WpmHistory.Add(wpm);
                    while (WpmHistory.Count > MaxWpmHistory) WpmHistory.RemoveAt(0);
                }
                break;
            case "char":
                if (!string.IsNullOrEmpty(ev.Ch) && !string.IsNullOrEmpty(ev.Morse))
                    Cells.Add(TranscriptCell.Char(ev.Ch!, ev.Morse!));
                break;
            case "word":
                Cells.Add(TranscriptCell.Word());
                break;
            case "garbled":
                // Per spec: don't show garbled morse in the message area.
                break;
            case "power":
                if (ev.Power is double p && ev.Threshold is double th)
                {
                    Power = p;
                    Threshold = th;
                    Signal = ev.Signal ?? false;
                    // Maintain a slowly-decaying ceiling so the meter auto-scales
                    // to the current signal envelope.
                    _powerCeiling = Math.Max(_powerCeiling * 0.9985, p);
                    if (_powerCeiling < 1e-9) _powerCeiling = 1e-9;
                    // Logarithmic mapping: ~60 dB dynamic range from the ceiling
                    // so weak vs strong is actually visible to the eye.
                    NormalizedLevel = LogNorm(p, _powerCeiling);
                    NormalizedThreshold = LogNorm(th, _powerCeiling);
                }
                break;
            case "end":
                StatusText = $"Done. {ev.Transcript ?? ""}";
                IsRunning = false;
                break;
        }
    }

    public void Dispose() => _process.Dispose();

    /// <summary>
    /// Map a linear power value into a 0..1 meter position using a 60 dB
    /// log scale referenced to the rolling ceiling. Anything ≥ ceiling
    /// reads as 1.0, anything ≤ ceiling/1000 reads as 0.0.
    /// </summary>
    private static double LogNorm(double v, double ceil)
    {
        if (v <= 0 || ceil <= 0) return 0;
        const double rangeDb = 60.0;
        double db = 10.0 * Math.Log10(v / ceil);
        double norm = 1.0 + db / rangeDb;
        return Math.Clamp(norm, 0.0, 1.0);
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private bool Set<T>(ref T field, T value, [CallerMemberName] string? name = null)
    {
        if (EqualityComparer<T>.Default.Equals(field, value)) return false;
        field = value;
        OnPropertyChanged(name);
        return true;
    }

    private void OnPropertyChanged([CallerMemberName] string? name = null)
        => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name!));
}
