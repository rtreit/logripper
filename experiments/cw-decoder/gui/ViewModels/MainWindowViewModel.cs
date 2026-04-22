using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.IO;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text.Json;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Threading;
using CwDecoderGui.Models;
using CwDecoderGui.Services;

namespace CwDecoderGui.ViewModels;

public sealed class MainWindowViewModel : INotifyPropertyChanged, IDisposable
{
    private readonly CwDecoderProcess _process = new();
    private CancellationTokenSource? _profileLoadCts;
    private readonly Dictionary<string, SignalProfile> _profileCache = new();
    private readonly Dictionary<string, CandidateDraftState> _candidateDrafts = new();

    public MainWindowViewModel()
    {
        Devices = new ObservableCollection<string>(CwDecoderProcess.ListDevices());
        SelectedDevice = Devices.Count > 0 ? Devices[0] : null;
        Cells = new ObservableCollection<TranscriptCell>();
        WpmHistory = new ObservableCollection<double>();
        HarvestCandidates = new ObservableCollection<HarvestCandidate>();

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
    public ObservableCollection<HarvestCandidate> HarvestCandidates { get; }

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
    public double PitchHz
    {
        get => _pitchHz;
        set { if (Set(ref _pitchHz, value)) OnPropertyChanged(nameof(SignalQualityLabel)); }
    }

    private double _power;
    public double Power { get => _power; set => Set(ref _power, value); }

    private double _threshold;
    public double Threshold { get => _threshold; set => Set(ref _threshold, value); }

    private bool _signal;
    public bool Signal { get => _signal; set => Set(ref _signal, value); }

    private double _snrDb;
    public double SnrDb
    {
        get => _snrDb;
        set { if (Set(ref _snrDb, value)) OnPropertyChanged(nameof(SignalQualityLabel)); }
    }

    private double _noise;
    public double Noise { get => _noise; set => Set(ref _noise, value); }

    private string? _statusText;
    public string? StatusText { get => _statusText; set => Set(ref _statusText, value); }

    private string _sourceLabel = "(idle)";
    public string SourceLabel { get => _sourceLabel; set => Set(ref _sourceLabel, value); }

    private double _normalizedLevel;
    public double NormalizedLevel { get => _normalizedLevel; set => Set(ref _normalizedLevel, value); }

    private double _normalizedThreshold;
    public double NormalizedThreshold { get => _normalizedThreshold; set => Set(ref _normalizedThreshold, value); }

    private double _minSnrDb = DecoderConfig.DefaultMinSnrDb;
    public double MinSnrDb
    {
        get => _minSnrDb;
        set { if (Set(ref _minSnrDb, value)) PushConfig(); }
    }

    private double _pitchMinSnrDb = DecoderConfig.DefaultPitchMinSnrDb;
    public double PitchMinSnrDb
    {
        get => _pitchMinSnrDb;
        set { if (Set(ref _pitchMinSnrDb, value)) PushConfig(); }
    }

    private double _thresholdScale = DecoderConfig.DefaultThresholdScale;
    public double ThresholdScale
    {
        get => _thresholdScale;
        set { if (Set(ref _thresholdScale, value)) PushConfig(); }
    }

    private string? _harvestFilePath;
    public string? HarvestFilePath { get => _harvestFilePath; set => Set(ref _harvestFilePath, value); }

    private string _harvestNeedlesText = string.Empty;
    public string HarvestNeedlesText { get => _harvestNeedlesText; set => Set(ref _harvestNeedlesText, value); }

    private double _harvestWindowSeconds = 4.0;
    public double HarvestWindowSeconds { get => _harvestWindowSeconds; set => Set(ref _harvestWindowSeconds, value); }

    private double _harvestHopSeconds = 1.0;
    public double HarvestHopSeconds { get => _harvestHopSeconds; set => Set(ref _harvestHopSeconds, value); }

    private double _previewSlowdown = 2.5;
    public double PreviewSlowdown { get => _previewSlowdown; set => Set(ref _previewSlowdown, value); }

    private bool _isAdvancedBusy;
    public bool IsAdvancedBusy
    {
        get => _isAdvancedBusy;
        set
        {
            if (Set(ref _isAdvancedBusy, value))
            {
                OnPropertyChanged(nameof(CanHarvestCandidates));
                OnPropertyChanged(nameof(CanPreviewCandidate));
                OnPropertyChanged(nameof(CanSaveLabel));
                OnPropertyChanged(nameof(CanResetAdjustedSpan));
                OnPropertyChanged(nameof(CanUseSuggestedSpan));
            }
        }
    }

    private bool _isHarvestBusy;
    public bool IsHarvestBusy
    {
        get => _isHarvestBusy;
        set => Set(ref _isHarvestBusy, value);
    }

    private double _harvestProgressValue;
    public double HarvestProgressValue
    {
        get => _harvestProgressValue;
        set => Set(ref _harvestProgressValue, value);
    }

    private double _harvestProgressMaximum = 1;
    public double HarvestProgressMaximum
    {
        get => _harvestProgressMaximum;
        set => Set(ref _harvestProgressMaximum, value);
    }

    private string _harvestProgressLabel = string.Empty;
    public string HarvestProgressLabel
    {
        get => _harvestProgressLabel;
        set => Set(ref _harvestProgressLabel, value);
    }

    private bool _isProfileBusy;
    public bool IsProfileBusy
    {
        get => _isProfileBusy;
        set
        {
            if (Set(ref _isProfileBusy, value))
            {
                OnPropertyChanged(nameof(CanPreviewCandidate));
                OnPropertyChanged(nameof(CanSaveLabel));
                OnPropertyChanged(nameof(CanResetAdjustedSpan));
                OnPropertyChanged(nameof(CanUseSuggestedSpan));
            }
        }
    }

    private string _advancedStatusText = "Pick an audio file, harvest windows, play a slowed preview, then save exact-window verified copy.";
    public string AdvancedStatusText { get => _advancedStatusText; set => Set(ref _advancedStatusText, value); }

    private HarvestCandidate? _selectedCandidate;
    public HarvestCandidate? SelectedCandidate
    {
        get => _selectedCandidate;
        set
        {
            if (!IsSameCandidate(_selectedCandidate, value))
            {
                PersistDraftForCandidate(_selectedCandidate);
            }

            if (Set(ref _selectedCandidate, value))
            {
                SignalProfile? cachedProfile = null;
                var hasCachedProfile = value is not null && TryGetCachedProfile(value, out cachedProfile);
                var draft = value is not null && TryGetDraftForCandidate(value, out var candidateDraft)
                    ? candidateDraft
                    : null;
                CurrentSignalProfile = hasCachedProfile && cachedProfile is not null
                    ? cachedProfile
                    : CreateEmptySignalProfile();
                CorrectCopy = draft?.CorrectCopy ?? string.Empty;
                ClipStart = draft?.ClipStart ?? false;
                ClipEnd = draft?.ClipEnd ?? false;
                SetAdjustedSpanInternal(
                    draft?.AdjustedStartSeconds ?? value?.StartSeconds ?? 0,
                    draft?.AdjustedEndSeconds ?? value?.EndSeconds ?? 0,
                    clampToProfile: false);
                OnPropertyChanged(nameof(SelectedCandidateRange));
                OnPropertyChanged(nameof(SelectedCandidateNeedles));
                OnPropertyChanged(nameof(AdjustedRangeLabel));
                OnPropertyChanged(nameof(CanPreviewCandidate));
                OnPropertyChanged(nameof(CanSaveLabel));
                OnPropertyChanged(nameof(CanResetAdjustedSpan));
                OnPropertyChanged(nameof(CanUseSuggestedSpan));
                if (value is not null && !hasCachedProfile)
                {
                    _ = LoadSelectedProfileAsync(value);
                }
            }
        }
    }

    private string _correctCopy = string.Empty;
    public string CorrectCopy
    {
        get => _correctCopy;
        set
        {
            var normalized = (value ?? string.Empty).ToUpperInvariant();
            if (Set(ref _correctCopy, normalized))
                OnPropertyChanged(nameof(CanSaveLabel));
        }
    }

    private bool _clipStart;
    public bool ClipStart { get => _clipStart; set => Set(ref _clipStart, value); }

    private bool _clipEnd;
    public bool ClipEnd { get => _clipEnd; set => Set(ref _clipEnd, value); }

    private SignalProfile _currentSignalProfile = CreateEmptySignalProfile();
    public SignalProfile CurrentSignalProfile
    {
        get => _currentSignalProfile;
        private set
        {
            if (Set(ref _currentSignalProfile, value))
            {
                ClampAdjustedSpanToProfile();
                OnPropertyChanged(nameof(CanUseSuggestedSpan));
                OnPropertyChanged(nameof(CanResetAdjustedSpan));
            }
        }
    }

    private double _adjustedStartSeconds;
    public double AdjustedStartSeconds
    {
        get => _adjustedStartSeconds;
        set => SetAdjustedSpanInternal(value, _adjustedEndSeconds, clampToProfile: true);
    }

    private double _adjustedEndSeconds;
    public double AdjustedEndSeconds
    {
        get => _adjustedEndSeconds;
        set => SetAdjustedSpanInternal(_adjustedStartSeconds, value, clampToProfile: true);
    }

    public string SignalQualityLabel
    {
        get
        {
            if (PitchHz <= 0) return "NO LOCK";
            if (SnrDb < MinSnrDb - 2) return "NOISE";
            if (SnrDb < MinSnrDb + 2) return "WEAK";
            if (SnrDb < MinSnrDb + 10) return "GOOD";
            return "STRONG";
        }
    }

    public string SelectedCandidateRange => SelectedCandidate?.RangeLabel ?? "(no candidate selected)";
    public string SelectedCandidateNeedles => SelectedCandidate?.NeedlesLabel ?? "-";
    public string AdjustedRangeLabel => SelectedCandidate is null
        ? "(no selection)"
        : $"{AdjustedStartSeconds:F2}s - {AdjustedEndSeconds:F2}s";
    public bool CanHarvestCandidates => !IsAdvancedBusy && !string.IsNullOrWhiteSpace(HarvestFilePath);
    public string LabelFilePath => string.IsNullOrWhiteSpace(HarvestFilePath)
        ? "(select an audio file)"
        : Path.ChangeExtension(HarvestFilePath, ".labels.jsonl");
    public bool CanPreviewCandidate => SelectedCandidate is not null
        && !string.IsNullOrWhiteSpace(HarvestFilePath)
        && !IsAdvancedBusy
        && !IsProfileBusy
        && AdjustedEndSeconds > AdjustedStartSeconds;
    public bool CanSaveLabel => SelectedCandidate is not null
        && !string.IsNullOrWhiteSpace(HarvestFilePath)
        && !string.IsNullOrWhiteSpace(CorrectCopy)
        && !IsAdvancedBusy
        && !IsProfileBusy
        && AdjustedEndSeconds > AdjustedStartSeconds;
    public bool CanResetAdjustedSpan => SelectedCandidate is not null
        && (Math.Abs(AdjustedStartSeconds - SelectedCandidate.StartSeconds) > 0.0005
            || Math.Abs(AdjustedEndSeconds - SelectedCandidate.EndSeconds) > 0.0005);
    public bool CanUseSuggestedSpan => SelectedCandidate is not null
        && CurrentSignalProfile.HasData
        && (Math.Abs(AdjustedStartSeconds - CurrentSignalProfile.SuggestedStartSeconds) > 0.0005
            || Math.Abs(AdjustedEndSeconds - CurrentSignalProfile.SuggestedEndSeconds) > 0.0005);

    public void ResetSensitivity()
    {
        MinSnrDb = DecoderConfig.DefaultMinSnrDb;
        PitchMinSnrDb = DecoderConfig.DefaultPitchMinSnrDb;
        ThresholdScale = DecoderConfig.DefaultThresholdScale;
    }

    private DecoderConfig CurrentConfig() => new(MinSnrDb, PitchMinSnrDb, ThresholdScale);

    private void PushConfig()
    {
        if (IsRunning) _process.SendConfig(CurrentConfig());
        OnPropertyChanged(nameof(SignalQualityLabel));
    }

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
        _process.StartLive(SelectedDevice, CurrentConfig());
        IsRunning = true;
    }

    public void OpenFile(string path)
    {
        if (IsRunning) { _process.Stop(); IsRunning = false; }
        Cells.Clear();
        WpmHistory.Clear();
        Wpm = 0;
        _powerCeiling = 1e-6;
        SetHarvestFile(path);
        StatusText = $"Decoding {path}";
        _process.StartFile(path, realtime: true, CurrentConfig());
        IsRunning = true;
    }

    public void SetHarvestFile(string path)
    {
        HarvestFilePath = path;
        HarvestCandidates.Clear();
        SelectedCandidate = null;
        _profileCache.Clear();
        _candidateDrafts.Clear();
        CurrentSignalProfile = CreateEmptySignalProfile();
        ResetHarvestProgress();
        AdvancedStatusText = $"Selected {Path.GetFileName(path)} for candidate harvest.";
        OnPropertyChanged(nameof(CanHarvestCandidates));
        OnPropertyChanged(nameof(LabelFilePath));
        OnPropertyChanged(nameof(CanPreviewCandidate));
        OnPropertyChanged(nameof(CanSaveLabel));
    }

    public async Task HarvestCandidatesAsync()
    {
        if (string.IsNullOrWhiteSpace(HarvestFilePath))
        {
            AdvancedStatusText = "Pick an audio file first.";
            return;
        }

        try
        {
            IsAdvancedBusy = true;
            IsHarvestBusy = true;
            HarvestProgressValue = 0;
            HarvestProgressMaximum = 1;
            HarvestProgressLabel = "Preparing harvest…";
            AdvancedStatusText = $"Harvesting candidate windows from {Path.GetFileName(HarvestFilePath)}…";
            var result = await _process.HarvestFileAsync(
                HarvestFilePath,
                HarvestWindowSeconds,
                HarvestHopSeconds,
                chunkMs: 50,
                top: 16,
                minSharedChars: 4,
                needles: ParseNeedles(HarvestNeedlesText),
                cfg: CurrentConfig(),
                onProgress: (completed, total, startSeconds, endSeconds) =>
                    Dispatcher.UIThread.Post(() => UpdateHarvestProgress(completed, total, startSeconds, endSeconds)))
                .ConfigureAwait(true);

            HarvestCandidates.Clear();
            foreach (var candidate in result.Candidates)
                HarvestCandidates.Add(candidate);
            SelectedCandidate = HarvestCandidates.FirstOrDefault();
            HarvestProgressValue = HarvestProgressMaximum;
            HarvestProgressLabel = HarvestCandidates.Count == 0
                ? "Harvest finished with no candidate matches."
                : $"Harvest finished: {HarvestCandidates.Count} candidate regions.";
            AdvancedStatusText = HarvestCandidates.Count == 0
                ? "No candidate windows matched the current filters."
                : $"Harvested {HarvestCandidates.Count} candidate windows.";
        }
        catch (Exception ex)
        {
            AdvancedStatusText = ex.Message;
        }
        finally
        {
            IsHarvestBusy = false;
            IsAdvancedBusy = false;
        }
    }

    public async Task PlaySelectedCandidateAsync()
    {
        if (SelectedCandidate is null || string.IsNullOrWhiteSpace(HarvestFilePath))
        {
            AdvancedStatusText = "Select a candidate window first.";
            return;
        }

        try
        {
            IsAdvancedBusy = true;
            AdvancedStatusText = $"Rendering slowed preview for {AdjustedRangeLabel}…";
            var previewPath = await _process.RenderPreviewAsync(
                HarvestFilePath,
                AdjustedStartSeconds,
                AdjustedEndSeconds - AdjustedStartSeconds,
                PreviewSlowdown).ConfigureAwait(true);
            CwDecoderProcess.OpenPreview(previewPath);
            AdvancedStatusText = $"Opened preview: {Path.GetFileName(previewPath)}";
        }
        catch (Exception ex)
        {
            AdvancedStatusText = ex.Message;
        }
        finally
        {
            IsAdvancedBusy = false;
        }
    }

    public void SaveSelectedLabel()
    {
        if (SelectedCandidate is null || string.IsNullOrWhiteSpace(HarvestFilePath) || string.IsNullOrWhiteSpace(CorrectCopy))
        {
            AdvancedStatusText = "Select a candidate and enter the verified copy first.";
            return;
        }

        var labelPath = LabelFilePath;
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(labelPath)!);
            var label = new CandidateLabel
            {
                Source = HarvestFilePath,
                StartSeconds = AdjustedStartSeconds,
                EndSeconds = AdjustedEndSeconds,
                HarvestStartSeconds = SelectedCandidate.StartSeconds,
                HarvestEndSeconds = SelectedCandidate.EndSeconds,
                LabelScope = CandidateLabel.ExactWindowScope,
                CorrectCopy = CorrectCopy.Trim(),
                ClipStart = ClipStart,
                ClipEnd = ClipEnd,
                Needles = SelectedCandidate.MatchedNeedles,
                OfflineText = SelectedCandidate.Offline.Text,
                StreamText = SelectedCandidate.Stream.Text,
                OfflinePitchHz = SelectedCandidate.Offline.PitchHz,
                StreamPitchHz = SelectedCandidate.Stream.PitchHz,
                OfflineWpm = SelectedCandidate.Offline.Wpm,
                StreamWpm = SelectedCandidate.Stream.Wpm,
                SavedAtUtc = DateTime.UtcNow.ToString("O"),
            };

            var lines = File.Exists(labelPath)
                ? File.ReadAllLines(labelPath).Where(line => !MatchesSameWindow(line, label)).ToList()
                : new List<string>();
            lines.Add(JsonSerializer.Serialize(label));
            File.WriteAllLines(labelPath, lines);
            _candidateDrafts[CandidateKey(SelectedCandidate)] = CandidateDraftState.FromLabel(label);
            AdvancedStatusText = $"Saved verified copy to {Path.GetFileName(labelPath)}.";
        }
        catch (Exception ex)
        {
            AdvancedStatusText = ex.Message;
        }
    }

    public void ResetAdjustedSpan()
    {
        if (SelectedCandidate is null)
        {
            return;
        }

        SetAdjustedSpanInternal(SelectedCandidate.StartSeconds, SelectedCandidate.EndSeconds, clampToProfile: true);
    }

    public void UseSuggestedSpan()
    {
        if (!CurrentSignalProfile.HasData)
        {
            return;
        }

        SetAdjustedSpanInternal(
            CurrentSignalProfile.SuggestedStartSeconds,
            CurrentSignalProfile.SuggestedEndSeconds,
            clampToProfile: true);
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
                break;
            case "power":
                if (ev.Power is double p && ev.Threshold is double th)
                {
                    Power = p;
                    Threshold = th;
                    Noise = ev.Noise ?? 0;
                    Signal = ev.Signal ?? false;
                    if (ev.Snr is double snrLin && snrLin > 0)
                        SnrDb = 10.0 * Math.Log10(snrLin);
                    _powerCeiling = Math.Max(_powerCeiling * 0.9985, p);
                    if (_powerCeiling < 1e-9) _powerCeiling = 1e-9;
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

    public void Dispose()
    {
        CancelAndDisposeProfileLoad();
        _process.Dispose();
    }

    private static string[] ParseNeedles(string? text)
    {
        return (text ?? string.Empty)
            .Split([' ', ',', ';', '\r', '\n', '\t'], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
            .Distinct(StringComparer.OrdinalIgnoreCase)
            .ToArray();
    }

    private async Task LoadSelectedProfileAsync(HarvestCandidate? candidate)
    {
        CancelAndDisposeProfileLoad();

        if (candidate is null || string.IsNullOrWhiteSpace(HarvestFilePath))
        {
            return;
        }

        var pitchHz = candidate.Stream.PitchHz ?? candidate.Offline.PitchHz;
        if (pitchHz is null || pitchHz <= 0)
        {
            AdvancedStatusText = "Selected candidate has no usable pitch estimate for profile rendering.";
            return;
        }

        var cts = new CancellationTokenSource();
        _profileLoadCts = cts;

        try
        {
            IsProfileBusy = true;
            AdvancedStatusText = $"Loading signal profile for {candidate.RangeLabel}…";
            var profile = await _process.LoadSignalProfileAsync(
                HarvestFilePath,
                candidate.StartSeconds,
                candidate.EndSeconds,
                pitchHz.Value,
                candidate.Stream.Wpm ?? candidate.Offline.Wpm,
                cts.Token).ConfigureAwait(true);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                if (cts.IsCancellationRequested || !IsSameCandidate(SelectedCandidate, candidate))
                {
                    return;
                }

                _profileCache[ProfileCacheKey(candidate)] = profile;
                CurrentSignalProfile = profile;
                AdvancedStatusText = "Drag the magenta handles to trim or extend the exact window, then preview or save.";
            });
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            if (!cts.IsCancellationRequested)
            {
                await Dispatcher.UIThread.InvokeAsync(() =>
                {
                    CurrentSignalProfile = CreateEmptySignalProfile();
                    AdvancedStatusText = ex.Message;
                });
            }
        }
        finally
        {
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                if (ReferenceEquals(_profileLoadCts, cts))
                {
                    _profileLoadCts = null;
                    IsProfileBusy = false;
                }
            });
            cts.Dispose();
        }
    }

    private void ClampAdjustedSpanToProfile()
    {
        if (!CurrentSignalProfile.HasData || SelectedCandidate is null)
        {
            return;
        }

        SetAdjustedSpanInternal(_adjustedStartSeconds, _adjustedEndSeconds, clampToProfile: true);
    }

    private void SetAdjustedSpanInternal(double startSeconds, double endSeconds, bool clampToProfile)
    {
        double minWidth = 0.08;
        double lowerBound = clampToProfile && CurrentSignalProfile.HasData
            ? CurrentSignalProfile.DisplayStartSeconds
            : Math.Min(startSeconds, endSeconds);
        double upperBound = clampToProfile && CurrentSignalProfile.HasData
            ? CurrentSignalProfile.DisplayEndSeconds
            : Math.Max(startSeconds, endSeconds);

        if (upperBound - lowerBound < minWidth)
        {
            upperBound = lowerBound + minWidth;
        }

        double clampedStart = Math.Clamp(startSeconds, lowerBound, upperBound - minWidth);
        double clampedEnd = Math.Clamp(endSeconds, clampedStart + minWidth, upperBound);

        bool changed = false;
        if (Math.Abs(_adjustedStartSeconds - clampedStart) > 0.0005)
        {
            _adjustedStartSeconds = clampedStart;
            OnPropertyChanged(nameof(AdjustedStartSeconds));
            changed = true;
        }
        if (Math.Abs(_adjustedEndSeconds - clampedEnd) > 0.0005)
        {
            _adjustedEndSeconds = clampedEnd;
            OnPropertyChanged(nameof(AdjustedEndSeconds));
            changed = true;
        }

        if (changed)
        {
            OnPropertyChanged(nameof(AdjustedRangeLabel));
            OnPropertyChanged(nameof(CanPreviewCandidate));
            OnPropertyChanged(nameof(CanSaveLabel));
            OnPropertyChanged(nameof(CanResetAdjustedSpan));
            OnPropertyChanged(nameof(CanUseSuggestedSpan));
        }
    }

    private bool TryGetCachedProfile(HarvestCandidate candidate, out SignalProfile profile)
        => _profileCache.TryGetValue(ProfileCacheKey(candidate), out profile!);

    private static SignalProfile CreateEmptySignalProfile() => new();

    private bool TryGetDraftForCandidate(HarvestCandidate candidate, out CandidateDraftState? draft)
    {
        if (_candidateDrafts.TryGetValue(CandidateKey(candidate), out var cachedDraft))
        {
            draft = cachedDraft;
            return true;
        }

        if (TryLoadSavedDraft(candidate, out var savedDraft))
        {
            _candidateDrafts[CandidateKey(candidate)] = savedDraft;
            draft = savedDraft;
            return true;
        }

        draft = null;
        return false;
    }

    private void PersistDraftForCandidate(HarvestCandidate? candidate)
    {
        if (candidate is null)
        {
            return;
        }

        _candidateDrafts[CandidateKey(candidate)] = new CandidateDraftState(
            _adjustedStartSeconds,
            _adjustedEndSeconds,
            _correctCopy,
            _clipStart,
            _clipEnd);
    }

    private bool TryLoadSavedDraft(HarvestCandidate candidate, out CandidateDraftState draft)
    {
        draft = null!;
        if (string.IsNullOrWhiteSpace(HarvestFilePath))
        {
            return false;
        }

        var labelPath = LabelFilePath;
        if (!File.Exists(labelPath))
        {
            return false;
        }

        foreach (var line in File.ReadLines(labelPath).Reverse())
        {
            try
            {
                var label = JsonSerializer.Deserialize<CandidateLabel>(line);
                if (label is not null && MatchesCandidate(label, candidate))
                {
                    draft = CandidateDraftState.FromLabel(label);
                    return true;
                }
            }
            catch
            {
            }
        }

        return false;
    }

    private void ResetHarvestProgress()
    {
        IsHarvestBusy = false;
        HarvestProgressValue = 0;
        HarvestProgressMaximum = 1;
        HarvestProgressLabel = string.Empty;
    }

    private void UpdateHarvestProgress(int completed, int total, double startSeconds, double endSeconds)
    {
        HarvestProgressMaximum = Math.Max(1, total);
        HarvestProgressValue = Math.Clamp(completed, 0, total > 0 ? total : 1);
        HarvestProgressLabel = total <= 0
            ? "Scanning candidate windows…"
            : $"Scanning {completed}/{total} · {startSeconds:F2}s - {endSeconds:F2}s";
    }

    private void CancelAndDisposeProfileLoad()
    {
        var previous = _profileLoadCts;
        _profileLoadCts = null;
        if (previous is null)
        {
            return;
        }

        try
        {
            previous.Cancel();
        }
        catch (ObjectDisposedException)
        {
        }

        previous.Dispose();
    }

    private static string ProfileCacheKey(HarvestCandidate candidate)
    {
        var pitch = candidate.Stream.PitchHz ?? candidate.Offline.PitchHz ?? 0;
        return $"{candidate.StartSeconds:F6}|{candidate.EndSeconds:F6}|{pitch:F3}";
    }

    private static string CandidateKey(HarvestCandidate candidate)
        => $"{candidate.StartSeconds:F6}|{candidate.EndSeconds:F6}";

    private static bool IsSameCandidate(HarvestCandidate? left, HarvestCandidate? right)
    {
        if (left is null || right is null)
        {
            return false;
        }

        return Math.Abs(left.StartSeconds - right.StartSeconds) < 0.0005
            && Math.Abs(left.EndSeconds - right.EndSeconds) < 0.0005;
    }

    private static bool MatchesSameWindow(string line, CandidateLabel label)
    {
        try
        {
            var existing = JsonSerializer.Deserialize<CandidateLabel>(line);
            if (existing?.HarvestStartSeconds is double existingHarvestStart
                && existing.HarvestEndSeconds is double existingHarvestEnd
                && label.HarvestStartSeconds is double labelHarvestStart
                && label.HarvestEndSeconds is double labelHarvestEnd)
            {
                return string.Equals(existing.Source, label.Source, StringComparison.OrdinalIgnoreCase)
                    && Math.Abs(existingHarvestStart - labelHarvestStart) < 0.0005
                    && Math.Abs(existingHarvestEnd - labelHarvestEnd) < 0.0005;
            }

            return existing is not null
                && string.Equals(existing.Source, label.Source, StringComparison.OrdinalIgnoreCase)
                && Math.Abs(existing.StartSeconds - label.StartSeconds) < 0.0005
                && Math.Abs(existing.EndSeconds - label.EndSeconds) < 0.0005;
        }
        catch
        {
            return false;
        }
    }

    private static bool MatchesCandidate(CandidateLabel label, HarvestCandidate candidate)
    {
        if (label.HarvestStartSeconds is double harvestStart && label.HarvestEndSeconds is double harvestEnd)
        {
            return Math.Abs(harvestStart - candidate.StartSeconds) < 0.0005
                && Math.Abs(harvestEnd - candidate.EndSeconds) < 0.0005;
        }

        return Math.Abs(label.StartSeconds - candidate.StartSeconds) < 0.0005
            && Math.Abs(label.EndSeconds - candidate.EndSeconds) < 0.0005;
    }

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

    private sealed record CandidateDraftState(
        double AdjustedStartSeconds,
        double AdjustedEndSeconds,
        string CorrectCopy,
        bool ClipStart,
        bool ClipEnd)
    {
        public static CandidateDraftState FromLabel(CandidateLabel label) => new(
            label.StartSeconds,
            label.EndSeconds,
            label.CorrectCopy ?? string.Empty,
            label.ClipStart,
            label.ClipEnd);
    }
}
