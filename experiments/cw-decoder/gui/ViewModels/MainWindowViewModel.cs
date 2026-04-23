using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Globalization;
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
    private CancellationTokenSource? _evaluationCts;
    private readonly Dictionary<string, SignalProfile> _profileCache = new();
    private readonly Dictionary<string, CandidateDraftState> _candidateDrafts = new();
    private readonly Dictionary<string, HarvestSessionState> _harvestSessionCache = new(StringComparer.OrdinalIgnoreCase);
    private SweepTopResult? _topSweepResult;

    private const string CustomDecoderModeLabel = "Custom streaming";
    private const string BaselineDecoderModeLabel = "Baseline ditdah";

    public MainWindowViewModel()
    {
        Devices = new ObservableCollection<string>(CwDecoderProcess.ListDevices());
        DecoderModes = new ObservableCollection<string>(new[] { CustomDecoderModeLabel, BaselineDecoderModeLabel });
        SelectedDevice = Devices.Count > 0 ? Devices[0] : null;
        SelectedDecoderMode = DecoderModes[0];
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
    public ObservableCollection<string> DecoderModes { get; }
    public ObservableCollection<TranscriptCell> Cells { get; }
    public ObservableCollection<double> WpmHistory { get; }
    public ObservableCollection<HarvestCandidate> HarvestCandidates { get; }

    private string? _selectedDevice;
    public string? SelectedDevice { get => _selectedDevice; set => Set(ref _selectedDevice, value); }

    private string _selectedDecoderMode = CustomDecoderModeLabel;
    public string SelectedDecoderMode
    {
        get => _selectedDecoderMode;
        set
        {
            if (Set(ref _selectedDecoderMode, value))
            {
                OnPropertyChanged(nameof(IsCustomDecoderMode));
                OnPropertyChanged(nameof(IsBaselineDecoderMode));
                OnPropertyChanged(nameof(BaselineDecoderSummary));
            }
        }
    }

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

    private string? _lastRecordingPath;
    public string? LastRecordingPath
    {
        get => _lastRecordingPath;
        set
        {
            if (Set(ref _lastRecordingPath, value))
            {
                OnPropertyChanged(nameof(HasLastRecording));
                OnPropertyChanged(nameof(LastRecordingDisplay));
            }
        }
    }

    public bool HasLastRecording => !string.IsNullOrEmpty(_lastRecordingPath) && System.IO.File.Exists(_lastRecordingPath);
    public string LastRecordingDisplay => string.IsNullOrEmpty(_lastRecordingPath) ? "" : System.IO.Path.GetFileName(_lastRecordingPath);

    private string? _liveTranscriptForReplay;
    private readonly System.Text.StringBuilder _liveTranscriptBuilder = new();
    private string? _replayTranscript;
    public string? ReplayTranscript { get => _replayTranscript; set => Set(ref _replayTranscript, value); }

    private string? _liveTranscriptDisplay;
    public string? LiveTranscriptDisplay { get => _liveTranscriptDisplay; set => Set(ref _liveTranscriptDisplay, value); }

    private string? _replayStatus;
    public string? ReplayStatus { get => _replayStatus; set => Set(ref _replayStatus, value); }

    private double? _replayCer;
    public double? ReplayCer
    {
        get => _replayCer;
        set
        {
            if (Set(ref _replayCer, value))
            {
                OnPropertyChanged(nameof(HasReplayCer));
                OnPropertyChanged(nameof(ReplayCerDisplay));
                OnPropertyChanged(nameof(ReplayCerForeground));
                OnPropertyChanged(nameof(ReplayCerBackground));
                OnPropertyChanged(nameof(ReplayGradeLabel));
            }
        }
    }

    public bool HasReplayCer => _replayCer.HasValue;
    public string ReplayCerDisplay => _replayCer is double c ? $"{c * 100:F1}%" : "—";
    public string ReplayGradeLabel => _replayCer switch
    {
        null => "",
        double c when c <= 0.05 => "EXCELLENT",
        double c when c <= 0.15 => "GOOD",
        double c when c <= 0.30 => "FAIR",
        double c when c <= 0.50 => "POOR",
        _ => "BAD",
    };
    public Avalonia.Media.IBrush ReplayCerForeground => _replayCer switch
    {
        null => Avalonia.Media.Brushes.Gray,
        double c when c <= 0.05 => Avalonia.Media.Brush.Parse("#7CFF7C"),
        double c when c <= 0.15 => Avalonia.Media.Brush.Parse("#B6FF7C"),
        double c when c <= 0.30 => Avalonia.Media.Brush.Parse("#FFD37C"),
        double c when c <= 0.50 => Avalonia.Media.Brush.Parse("#FF9F50"),
        _ => Avalonia.Media.Brush.Parse("#FF6464"),
    };
    public Avalonia.Media.IBrush ReplayCerBackground => _replayCer switch
    {
        null => Avalonia.Media.Brushes.Transparent,
        double c when c <= 0.05 => Avalonia.Media.Brush.Parse("#0E2A14"),
        double c when c <= 0.15 => Avalonia.Media.Brush.Parse("#15281A"),
        double c when c <= 0.30 => Avalonia.Media.Brush.Parse("#2A2415"),
        double c when c <= 0.50 => Avalonia.Media.Brush.Parse("#2A1A12"),
        _ => Avalonia.Media.Brush.Parse("#2A1010"),
    };

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

    private bool _evaluateAllLabels = true;
    public bool EvaluateAllLabels
    {
        get => _evaluateAllLabels;
        set
        {
            if (Set(ref _evaluateAllLabels, value))
            {
                OnPropertyChanged(nameof(LabelEvaluationTargetLabel));
                OnPropertyChanged(nameof(CanRunLabelScore));
                OnPropertyChanged(nameof(CanRunLabelSweep));
            }
        }
    }

    private bool _useWideSweep;
    public bool UseWideSweep { get => _useWideSweep; set => Set(ref _useWideSweep, value); }

    private bool _useFullStreamScorer;
    public bool UseFullStreamScorer { get => _useFullStreamScorer; set => Set(ref _useFullStreamScorer, value); }

    private double _labelEvalWindowSeconds = 20.0;
    public double LabelEvalWindowSeconds
    {
        get => _labelEvalWindowSeconds;
        set
        {
            if (Set(ref _labelEvalWindowSeconds, value))
            {
                OnPropertyChanged(nameof(BaselineDecoderSummary));
            }
        }
    }

    private double _labelEvalMinWindowSeconds = 0.5;
    public double LabelEvalMinWindowSeconds
    {
        get => _labelEvalMinWindowSeconds;
        set
        {
            if (Set(ref _labelEvalMinWindowSeconds, value))
            {
                OnPropertyChanged(nameof(BaselineDecoderSummary));
            }
        }
    }

    private double _labelEvalDecodeEveryMs = 1000;
    public double LabelEvalDecodeEveryMs
    {
        get => _labelEvalDecodeEveryMs;
        set
        {
            if (Set(ref _labelEvalDecodeEveryMs, value))
            {
                OnPropertyChanged(nameof(BaselineDecoderSummary));
            }
        }
    }

    private double _labelEvalConfirmations = 3;
    public double LabelEvalConfirmations
    {
        get => _labelEvalConfirmations;
        set
        {
            if (Set(ref _labelEvalConfirmations, value))
            {
                OnPropertyChanged(nameof(BaselineDecoderSummary));
            }
        }
    }

    private double _labelEvalTopResults = 10;
    public double LabelEvalTopResults { get => _labelEvalTopResults; set => Set(ref _labelEvalTopResults, value); }

    private double _labelEvalPreRollMs;
    public double LabelEvalPreRollMs { get => _labelEvalPreRollMs; set => Set(ref _labelEvalPreRollMs, value); }

    private double _labelEvalPostRollMs;
    public double LabelEvalPostRollMs { get => _labelEvalPostRollMs; set => Set(ref _labelEvalPostRollMs, value); }

    private bool _isEvaluationBusy;
    public bool IsEvaluationBusy
    {
        get => _isEvaluationBusy;
        set
        {
            if (Set(ref _isEvaluationBusy, value))
            {
                OnPropertyChanged(nameof(CanRunLabelScore));
                OnPropertyChanged(nameof(CanRunLabelSweep));
                OnPropertyChanged(nameof(CanApplyTopSweep));
            }
        }
    }

    private string _labelEvaluationStatusText = "Run label scoring or a parameter sweep to tune the causal ditdah baseline against saved labels.";
    public string LabelEvaluationStatusText { get => _labelEvaluationStatusText; set => Set(ref _labelEvaluationStatusText, value); }

    private string _labelEvaluationOutputText = "No label evaluation run yet.";
    public string LabelEvaluationOutputText { get => _labelEvaluationOutputText; set => Set(ref _labelEvaluationOutputText, value); }

    public bool IsCustomDecoderMode => string.Equals(SelectedDecoderMode, CustomDecoderModeLabel, StringComparison.Ordinal);
    public bool IsBaselineDecoderMode => string.Equals(SelectedDecoderMode, BaselineDecoderModeLabel, StringComparison.Ordinal);
    public string BaselineDecoderSummary => $"Baseline uses Tuning settings: {CurrentBaselineConfig().WindowSeconds:F1}s window / {CurrentBaselineConfig().MinWindowSeconds:F1}s min / {CurrentBaselineConfig().DecodeEveryMs}ms cadence / {CurrentBaselineConfig().Confirmations} confirmations.";
    public bool CanApplyTopSweep => _topSweepResult is not null && !IsEvaluationBusy;
    public string TopSweepSummary => _topSweepResult is null
        ? "Run a sweep to capture the best baseline settings for quick A/B testing on the Decoder tab."
        : $"Top sweep result: {_topSweepResult.WindowSeconds:F1}s window / {_topSweepResult.MinWindowSeconds:F1}s min / {_topSweepResult.DecodeEveryMs}ms cadence / {_topSweepResult.Confirmations} confirmations.";

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
                OnPropertyChanged(nameof(CanRunLabelScore));
                OnPropertyChanged(nameof(CanRunLabelSweep));
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
                OnPropertyChanged(nameof(CanRunLabelScore));
                OnPropertyChanged(nameof(CanRunLabelSweep));
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
    public string LabelEvaluationTargetLabel => EvaluateAllLabels
        ? @"Corpus: all labels under data\cw-samples"
        : $"Corpus: {LabelFilePath}";
    public bool CanRunLabelScore => !IsAdvancedBusy
        && !IsProfileBusy
        && !IsEvaluationBusy
        && HasLabelEvaluationTarget();
    public bool CanRunLabelSweep => !IsAdvancedBusy
        && !IsProfileBusy
        && !IsEvaluationBusy
        && HasLabelEvaluationTarget();

    public void ResetSensitivity()
    {
        MinSnrDb = DecoderConfig.DefaultMinSnrDb;
        PitchMinSnrDb = DecoderConfig.DefaultPitchMinSnrDb;
        ThresholdScale = DecoderConfig.DefaultThresholdScale;
    }

    private DecoderConfig CurrentConfig() => new(MinSnrDb, PitchMinSnrDb, ThresholdScale);
    private BaselineDecoderConfig CurrentBaselineConfig() => new(
        WindowSeconds: LabelEvalWindowSeconds,
        MinWindowSeconds: LabelEvalMinWindowSeconds,
        DecodeEveryMs: Math.Max(100, (int)Math.Round(LabelEvalDecodeEveryMs)),
        Confirmations: Math.Max(1, (int)Math.Round(LabelEvalConfirmations)));

    private void PushConfig()
    {
        if (IsRunning && IsCustomDecoderMode) _process.SendConfig(CurrentConfig());
        OnPropertyChanged(nameof(SignalQualityLabel));
    }

    private const int MaxWpmHistory = 200;
    private double _powerCeiling = 1e-6;

    private void ResetDecoderSurface()
    {
        Cells.Clear();
        WpmHistory.Clear();
        Wpm = 0;
        PitchHz = 0;
        Power = 0;
        Threshold = 0;
        Noise = 0;
        Signal = false;
        SnrDb = 0;
        NormalizedLevel = 0;
        NormalizedThreshold = 0;
        _powerCeiling = 1e-6;
    }

    public void ToggleStartStop()
    {
        if (IsRunning)
        {
            // Snapshot whatever we accumulated from char/word events before
            // killing the process — Stop won't reliably wait for the `end` JSON.
            _liveTranscriptForReplay = _liveTranscriptBuilder.ToString();
            _process.Stop();
            IsRunning = false;
            // Give the WAV writer a moment to flush via Drop, then refresh button.
            _ = Task.Run(async () =>
            {
                await Task.Delay(300).ConfigureAwait(false);
                await Dispatcher.UIThread.InvokeAsync(() => OnPropertyChanged(nameof(HasLastRecording)));
            });
            return;
        }
        ResetDecoderSurface();
        StatusText = "Starting…";

        // Generate timestamped recording path under <repo>/data/cw-recordings/
        string? recordPath = null;
        try
        {
            var recDir = LocateRecordingsDirectory();
            System.IO.Directory.CreateDirectory(recDir);
            recordPath = System.IO.Path.Combine(recDir, $"live-{DateTime.Now:yyyyMMdd-HHmmss}.wav");
        }
        catch (Exception ex)
        {
            StatusText = $"Recording disabled: {ex.Message}";
        }

        _liveTranscriptForReplay = null;
        _liveTranscriptBuilder.Clear();
        ReplayTranscript = null;
        ReplayStatus = null;
        ReplayCer = null;

        _process.StartLive(SelectedDevice, CurrentConfig(), CurrentBaselineConfig(), IsBaselineDecoderMode, recordPath);
        IsRunning = true;
    }

    private static string LocateRecordingsDirectory()
    {
        var dir = new System.IO.DirectoryInfo(AppContext.BaseDirectory);
        for (int i = 0; dir is not null && i < 8; i++, dir = dir.Parent)
        {
            var candidate = System.IO.Path.Combine(dir.FullName, "data", "cw-recordings");
            // Anchor on a directory we know is in the repo
            if (System.IO.Directory.Exists(System.IO.Path.Combine(dir.FullName, "data")))
            {
                return candidate;
            }
            // Or where the experiments folder lives
            if (System.IO.Directory.Exists(System.IO.Path.Combine(dir.FullName, "experiments", "cw-decoder")))
            {
                return System.IO.Path.Combine(dir.FullName, "data", "cw-recordings");
            }
        }
        return System.IO.Path.Combine(AppContext.BaseDirectory, "cw-recordings");
    }

    public async Task ReplayLastRecordingAsync()
    {
        var path = LastRecordingPath;
        if (string.IsNullOrEmpty(path) || !System.IO.File.Exists(path))
        {
            ReplayStatus = "No recording available.";
            return;
        }

        ReplayStatus = $"Re-decoding {System.IO.Path.GetFileName(path)} offline…";
        ReplayTranscript = null;
        ReplayCer = null;

        // Snapshot the live transcript right now in case the user clicks
        // Replay before the `end` event arrives (or after Stop killed it).
        var liveSnapshot = !string.IsNullOrWhiteSpace(_liveTranscriptForReplay)
            ? _liveTranscriptForReplay
            : _liveTranscriptBuilder.ToString();
        LiveTranscriptDisplay = string.IsNullOrWhiteSpace(liveSnapshot) ? "(empty)" : liveSnapshot.Trim();

        try
        {
            var transcript = await Task.Run(() => RunOfflineReplay(path)).ConfigureAwait(false);
            await Dispatcher.UIThread.InvokeAsync(() =>
            {
                ReplayTranscript = string.IsNullOrWhiteSpace(transcript) ? "(empty)" : transcript.Trim();
                if (!string.IsNullOrWhiteSpace(liveSnapshot) && !string.IsNullOrWhiteSpace(transcript))
                {
                    var live = liveSnapshot!.Trim();
                    var off = transcript.Trim();
                    var cer = CharacterErrorRate(off, live); // reference = offline (more reliable), hyp = live
                    ReplayCer = cer;
                    var lenL = live.Length;
                    var lenO = off.Length;
                    var coverage = lenO > 0 ? (double)lenL / lenO : 0.0;
                    ReplayStatus = $"Live vs offline · live={lenL} ch · offline={lenO} ch · coverage={coverage:P0} · CER={cer:P1}";
                }
                else if (!string.IsNullOrWhiteSpace(transcript))
                {
                    ReplayStatus = $"Offline transcript ready ({transcript.Trim().Length} chars). No live transcript captured to score against.";
                }
                else
                {
                    ReplayStatus = "Offline transcript was empty — recording may be silent or pitch lock failed.";
                }
            });
        }
        catch (Exception ex)
        {
            await Dispatcher.UIThread.InvokeAsync(() => ReplayStatus = $"Replay failed: {ex.Message}");
        }
    }

    private static string RunOfflineReplay(string wavPath)
    {
        var exeEnv = Environment.GetEnvironmentVariable("CW_DECODER_EXE");
        string? exe = (!string.IsNullOrWhiteSpace(exeEnv) && System.IO.File.Exists(exeEnv)) ? exeEnv : null;
        if (exe is null)
        {
            var name = OperatingSystem.IsWindows() ? "cw-decoder.exe" : "cw-decoder";
            var dir = new System.IO.DirectoryInfo(AppContext.BaseDirectory);
            for (int i = 0; dir is not null && i < 8 && exe is null; i++, dir = dir.Parent)
            {
                foreach (var rel in new[]
                {
                    System.IO.Path.Combine("target", "release", name),
                    System.IO.Path.Combine("target", "debug", name),
                    System.IO.Path.Combine("experiments", "cw-decoder", "target", "release", name),
                    System.IO.Path.Combine("experiments", "cw-decoder", "target", "debug", name),
                })
                {
                    var p = System.IO.Path.Combine(dir.FullName, rel);
                    if (System.IO.File.Exists(p)) { exe = p; break; }
                }
            }
        }
        if (exe is null) throw new InvalidOperationException("cw-decoder.exe not found.");

        var psi = new System.Diagnostics.ProcessStartInfo(exe)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        psi.ArgumentList.Add("stream-file-ditdah");
        psi.ArgumentList.Add("--json");
        psi.ArgumentList.Add("--chunk-ms");
        psi.ArgumentList.Add("50");
        psi.ArgumentList.Add(wavPath);

        using var proc = System.Diagnostics.Process.Start(psi)
            ?? throw new InvalidOperationException("Failed to start cw-decoder.");
        var stdout = proc.StandardOutput.ReadToEnd();
        proc.WaitForExit(180_000);
        return ExtractEndTranscript(stdout);
    }

    private static string ExtractEndTranscript(string stdout)
    {
        // Walk NDJSON lines and pick the `end` event's transcript.
        string? lastTranscript = null;
        foreach (var raw in stdout.Split('\n'))
        {
            var line = raw.TrimEnd('\r');
            if (string.IsNullOrWhiteSpace(line)) continue;
            try
            {
                using var doc = System.Text.Json.JsonDocument.Parse(line);
                var root = doc.RootElement;
                if (root.TryGetProperty("type", out var type) && type.GetString() == "end" &&
                    root.TryGetProperty("transcript", out var tx))
                {
                    lastTranscript = tx.GetString();
                }
            }
            catch (System.Text.Json.JsonException) { /* tolerate non-JSON noise */ }
        }
        return lastTranscript?.Trim() ?? string.Empty;
    }

    private static double CharacterErrorRate(string reference, string hypothesis)
    {
        if (reference.Length == 0) return hypothesis.Length == 0 ? 0.0 : 1.0;
        var dp = new int[reference.Length + 1, hypothesis.Length + 1];
        for (int i = 0; i <= reference.Length; i++) dp[i, 0] = i;
        for (int j = 0; j <= hypothesis.Length; j++) dp[0, j] = j;
        for (int i = 1; i <= reference.Length; i++)
        {
            for (int j = 1; j <= hypothesis.Length; j++)
            {
                var cost = reference[i - 1] == hypothesis[j - 1] ? 0 : 1;
                dp[i, j] = Math.Min(Math.Min(dp[i - 1, j] + 1, dp[i, j - 1] + 1), dp[i - 1, j - 1] + cost);
            }
        }
        return (double)dp[reference.Length, hypothesis.Length] / reference.Length;
    }

    public void OpenFile(string path)
    {
        if (IsRunning) { _process.Stop(); IsRunning = false; }
        ResetDecoderSurface();
        SetHarvestFile(path);
        StatusText = $"Decoding {path}";
        _process.StartFile(path, realtime: true, CurrentConfig(), CurrentBaselineConfig(), IsBaselineDecoderMode);
        IsRunning = true;
    }

    public void SetHarvestFile(string path)
    {
        if (string.Equals(HarvestFilePath, path, StringComparison.OrdinalIgnoreCase))
        {
            AdvancedStatusText = HarvestCandidates.Count > 0
                ? $"Reusing cached harvest for {Path.GetFileName(path)}. Click HARVEST to rescan."
                : $"Selected {Path.GetFileName(path)} for candidate harvest.";
            return;
        }

        SaveCurrentHarvestSession();
        HarvestFilePath = path;
        RestoreHarvestSession(path);
        ResetHarvestProgress();
        AdvancedStatusText = HarvestCandidates.Count > 0
            ? $"Restored cached harvest for {Path.GetFileName(path)}. Click HARVEST to rescan."
            : $"Selected {Path.GetFileName(path)} for candidate harvest.";
        OnPropertyChanged(nameof(CanHarvestCandidates));
        OnPropertyChanged(nameof(LabelFilePath));
        OnPropertyChanged(nameof(LabelEvaluationTargetLabel));
        OnPropertyChanged(nameof(CanPreviewCandidate));
        OnPropertyChanged(nameof(CanSaveLabel));
        OnPropertyChanged(nameof(CanRunLabelScore));
        OnPropertyChanged(nameof(CanRunLabelSweep));
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
            SaveCurrentHarvestSession();
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
            SaveCurrentHarvestSession();
            AdvancedStatusText = $"Saved verified copy to {Path.GetFileName(labelPath)}.";
            OnPropertyChanged(nameof(CanRunLabelScore));
            OnPropertyChanged(nameof(CanRunLabelSweep));
            OnPropertyChanged(nameof(LabelEvaluationTargetLabel));
        }
        catch (Exception ex)
        {
            AdvancedStatusText = ex.Message;
        }
    }

    public async Task RunLabelScoreAsync()
    {
        if (!TryResolveLabelEvaluationTarget(out var labelPath))
        {
            return;
        }

        CancelAndDisposeEvaluation();
        var cts = new CancellationTokenSource();
        _evaluationCts = cts;

        try
        {
            IsAdvancedBusy = true;
            IsEvaluationBusy = true;
            LabelEvaluationStatusText = EvaluateAllLabels
                ? "Scoring the full label corpus…"
                : $"Scoring {Path.GetFileName(labelPath)}…";
            LabelEvaluationOutputText = string.Empty;
            var output = await _process.RunLabelScoreAsync(
                EvaluateAllLabels,
                labelPath,
                UseFullStreamScorer,
                Math.Max(0, (int)Math.Round(LabelEvalPreRollMs)),
                Math.Max(0, (int)Math.Round(LabelEvalPostRollMs)),
                LabelEvalWindowSeconds,
                LabelEvalMinWindowSeconds,
                Math.Max(100, (int)Math.Round(LabelEvalDecodeEveryMs)),
                Math.Max(1, (int)Math.Round(LabelEvalConfirmations)),
                cts.Token).ConfigureAwait(true);
            LabelEvaluationOutputText = output.Trim();
            LabelEvaluationStatusText = EvaluateAllLabels
                ? "Finished scoring the full label corpus."
                : $"Finished scoring {Path.GetFileName(labelPath)}.";
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            LabelEvaluationStatusText = ex.Message;
        }
        finally
        {
            if (ReferenceEquals(_evaluationCts, cts))
            {
                _evaluationCts = null;
            }
            cts.Dispose();
            IsEvaluationBusy = false;
            IsAdvancedBusy = false;
        }
    }

    public async Task RunLabelSweepAsync()
    {
        if (!TryResolveLabelEvaluationTarget(out var labelPath))
        {
            return;
        }

        CancelAndDisposeEvaluation();
        var cts = new CancellationTokenSource();
        _evaluationCts = cts;

        try
        {
            IsAdvancedBusy = true;
            IsEvaluationBusy = true;
            LabelEvaluationStatusText = UseWideSweep
                ? "Running wide parameter sweep…"
                : "Running interactive parameter sweep…";
            LabelEvaluationOutputText = string.Empty;
            _topSweepResult = null;
            OnPropertyChanged(nameof(CanApplyTopSweep));
            OnPropertyChanged(nameof(TopSweepSummary));
            var output = await _process.RunLabelSweepAsync(
                EvaluateAllLabels,
                labelPath,
                UseFullStreamScorer,
                Math.Max(0, (int)Math.Round(LabelEvalPreRollMs)),
                Math.Max(0, (int)Math.Round(LabelEvalPostRollMs)),
                UseWideSweep,
                Math.Max(1, (int)Math.Round(LabelEvalTopResults)),
                cts.Token).ConfigureAwait(true);
            LabelEvaluationOutputText = output.Trim();
            _topSweepResult = TryParseTopSweepResult(LabelEvaluationOutputText);
            OnPropertyChanged(nameof(CanApplyTopSweep));
            OnPropertyChanged(nameof(TopSweepSummary));
            LabelEvaluationStatusText = UseWideSweep
                ? "Finished wide parameter sweep."
                : "Finished interactive parameter sweep.";
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception ex)
        {
            LabelEvaluationStatusText = ex.Message;
        }
        finally
        {
            if (ReferenceEquals(_evaluationCts, cts))
            {
                _evaluationCts = null;
            }
            cts.Dispose();
            IsEvaluationBusy = false;
            IsAdvancedBusy = false;
        }
    }

    public void ApplyTopSweepResult()
    {
        if (_topSweepResult is null)
        {
            LabelEvaluationStatusText = "Run a sweep first to get an applied baseline candidate.";
            return;
        }

        LabelEvalWindowSeconds = _topSweepResult.WindowSeconds;
        LabelEvalMinWindowSeconds = _topSweepResult.MinWindowSeconds;
        LabelEvalDecodeEveryMs = _topSweepResult.DecodeEveryMs;
        LabelEvalConfirmations = _topSweepResult.Confirmations;
        LabelEvaluationStatusText = "Applied the top sweep result to the shared baseline tuning settings. Decoder tab baseline mode now uses these values.";
        OnPropertyChanged(nameof(BaselineDecoderSummary));
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
                SourceLabel = ev.Source switch
                {
                    "live" => $"LIVE · {ev.Device} · {ev.Rate} Hz",
                    "live-baseline" => $"LIVE BASELINE · {ev.Device} · {ev.Rate} Hz",
                    "file-baseline" => $"FILE BASELINE · {System.IO.Path.GetFileName(ev.Path ?? "?")}",
                    _ => $"FILE · {System.IO.Path.GetFileName(ev.Path ?? "?")}",
                };
                StatusText = ev.Source is "live-baseline" or "file-baseline"
                    ? "Running baseline decode snapshots…"
                    : "Listening for pitch lock…";
                if (!string.IsNullOrEmpty(ev.Recording))
                {
                    LastRecordingPath = ev.Recording;
                }
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
                    WpmHistory.Add(wpm);
                    while (WpmHistory.Count > MaxWpmHistory) WpmHistory.RemoveAt(0);
                    // Big number = short rolling average (3 snapshots ≈ 1.5s
                    // at decode_every_ms=500) so it doesn't bounce wildly on
                    // every snapshot but still tracks real WPM changes
                    // promptly. Sparkline still shows raw history.
                    int avgWindow = Math.Min(WpmHistory.Count, 3);
                    if (avgWindow > 0)
                    {
                        double sum = 0;
                        for (int i = WpmHistory.Count - avgWindow; i < WpmHistory.Count; i++) sum += WpmHistory[i];
                        Wpm = sum / avgWindow;
                    }
                }
                break;
            case "char":
                if (!string.IsNullOrEmpty(ev.Ch))
                {
                    Cells.Add(TranscriptCell.Char(ev.Ch!, string.IsNullOrEmpty(ev.Morse) ? " " : ev.Morse!));
                    _liveTranscriptBuilder.Append(ev.Ch);
                }
                break;
            case "word":
                Cells.Add(TranscriptCell.Word());
                if (_liveTranscriptBuilder.Length > 0 && _liveTranscriptBuilder[^1] != ' ')
                    _liveTranscriptBuilder.Append(' ');
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
                _liveTranscriptForReplay = !string.IsNullOrWhiteSpace(ev.Transcript)
                    ? ev.Transcript
                    : _liveTranscriptBuilder.ToString();
                if (!string.IsNullOrEmpty(ev.Recording))
                {
                    LastRecordingPath = ev.Recording;
                }
                else
                {
                    OnPropertyChanged(nameof(HasLastRecording));
                }
                IsRunning = false;
                break;
        }
    }

    public void Dispose()
    {
        CancelAndDisposeProfileLoad();
        CancelAndDisposeEvaluation();
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

    private void SaveCurrentHarvestSession()
    {
        if (string.IsNullOrWhiteSpace(HarvestFilePath))
        {
            return;
        }

        PersistDraftForCandidate(SelectedCandidate);
        _harvestSessionCache[HarvestFilePath] = new HarvestSessionState(
            HarvestCandidates.ToList(),
            SelectedCandidate is null ? null : CandidateKey(SelectedCandidate),
            new Dictionary<string, SignalProfile>(_profileCache),
            new Dictionary<string, CandidateDraftState>(_candidateDrafts));
    }

    private void RestoreHarvestSession(string path)
    {
        HarvestCandidates.Clear();
        SelectedCandidate = null;
        _profileCache.Clear();
        _candidateDrafts.Clear();
        CurrentSignalProfile = CreateEmptySignalProfile();

        if (!_harvestSessionCache.TryGetValue(path, out var session))
        {
            return;
        }

        foreach (var candidate in session.Candidates)
        {
            HarvestCandidates.Add(candidate);
        }

        foreach (var pair in session.ProfileCache)
        {
            _profileCache[pair.Key] = pair.Value;
        }

        foreach (var pair in session.CandidateDrafts)
        {
            _candidateDrafts[pair.Key] = pair.Value;
        }

        SelectedCandidate = session.SelectedCandidateKey is null
            ? HarvestCandidates.FirstOrDefault()
            : HarvestCandidates.FirstOrDefault(candidate => CandidateKey(candidate) == session.SelectedCandidateKey)
                ?? HarvestCandidates.FirstOrDefault();
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

    private void CancelAndDisposeEvaluation()
    {
        var previous = _evaluationCts;
        _evaluationCts = null;
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

    private static SweepTopResult? TryParseTopSweepResult(string output)
    {
        foreach (var rawLine in output.Split('\n'))
        {
            var line = rawLine.Trim();
            if (string.IsNullOrWhiteSpace(line) || !char.IsDigit(line[0]) || !line.Contains('/'))
            {
                continue;
            }

            var parts = line.Split(' ', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
            if (parts.Length < 7)
            {
                continue;
            }

            if (double.TryParse(parts[3], NumberStyles.Float | NumberStyles.AllowThousands, CultureInfo.InvariantCulture, out var windowSeconds)
                && double.TryParse(parts[4], NumberStyles.Float | NumberStyles.AllowThousands, CultureInfo.InvariantCulture, out var minWindowSeconds)
                && int.TryParse(parts[5], NumberStyles.Integer, CultureInfo.InvariantCulture, out var decodeEveryMs)
                && int.TryParse(parts[6], NumberStyles.Integer, CultureInfo.InvariantCulture, out var confirmations))
            {
                return new SweepTopResult(windowSeconds, minWindowSeconds, decodeEveryMs, confirmations);
            }
        }

        return null;
    }

    private bool HasLabelEvaluationTarget()
    {
        if (EvaluateAllLabels)
        {
            return true;
        }

        return !string.IsNullOrWhiteSpace(HarvestFilePath) && File.Exists(LabelFilePath);
    }

    private bool TryResolveLabelEvaluationTarget(out string? labelPath)
    {
        labelPath = null;
        if (EvaluateAllLabels)
        {
            return true;
        }

        if (string.IsNullOrWhiteSpace(HarvestFilePath))
        {
            LabelEvaluationStatusText = "Pick an audio file first, or enable ALL LABELS.";
            return false;
        }

        labelPath = LabelFilePath;
        if (!File.Exists(labelPath))
        {
            LabelEvaluationStatusText = $"No saved labels yet at {Path.GetFileName(labelPath)}.";
            return false;
        }

        return true;
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

    private sealed record HarvestSessionState(
        IReadOnlyList<HarvestCandidate> Candidates,
        string? SelectedCandidateKey,
        Dictionary<string, SignalProfile> ProfileCache,
        Dictionary<string, CandidateDraftState> CandidateDrafts);

    private sealed record SweepTopResult(
        double WindowSeconds,
        double MinWindowSeconds,
        int DecodeEveryMs,
        int Confirmations);
}
