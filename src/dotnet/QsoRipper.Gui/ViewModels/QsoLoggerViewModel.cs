using System;
using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

/// <summary>
/// ViewModel for the QSO creation panel. Manages callsign entry, band/mode
/// cycling, elapsed-time tracking, and submission to the engine.
/// </summary>
internal sealed partial class QsoLoggerViewModel : ObservableObject
{
    private readonly IEngineClient _engine;
    private readonly DispatcherTimer _elapsedTimer;
    private CwQsoWpmAggregator? _cwWpmAggregator;
    private CwQsoTranscriptAggregator? _cwTranscriptAggregator;
    private Action? _cwResetLockHandler;
    private DateTimeOffset _qsoStartTime;
    private bool _timerRunning;
    private CancellationTokenSource? _lookupCts;

    // Manual-override tracking: when true the field was explicitly typed by
    // the operator and should not be overwritten by band/mode defaults or
    // rig snapshots.
    private bool _frequencyManuallySet;
    private bool _rstManuallySet;
    private bool _bandManuallySet;
    private bool _modeManuallySet;
    private CallsignRecord? _lastLookupRecord;

    // ── Observable properties ────────────────────────────────────────────

    [ObservableProperty]
    private string _callsign = string.Empty;

    [ObservableProperty]
    private int _selectedBandIndex;

    [ObservableProperty]
    private int _selectedModeIndex;

    [ObservableProperty]
    private string _rstSent = "59";

    [ObservableProperty]
    private string _rstRcvd = "59";

    [ObservableProperty]
    private string _frequencyMhz = "14.225";

    [ObservableProperty]
    private string _comment = string.Empty;

    [ObservableProperty]
    private string _notes = string.Empty;

    [ObservableProperty]
    private string _contestId = string.Empty;

    [ObservableProperty]
    private string _exchangeSent = string.Empty;

    [ObservableProperty]
    private string _elapsedTimeText = "00:00";

    [ObservableProperty]
    private bool _isLogEnabled;

    [ObservableProperty]
    private string _logStatusText = string.Empty;

    [ObservableProperty]
    private string _lookupName = string.Empty;

    [ObservableProperty]
    private string _lookupGrid = string.Empty;

    [ObservableProperty]
    private string _lookupCountry = string.Empty;

    [ObservableProperty]
    private string _lookupStatusText = string.Empty;

    // ── Constructor ──────────────────────────────────────────────────────

    public QsoLoggerViewModel(IEngineClient engine)
        : this(engine, cwWpmAggregator: null)
    {
    }

    internal QsoLoggerViewModel(IEngineClient engine, CwQsoWpmAggregator? cwWpmAggregator)
    {
        _engine = engine;
        _cwWpmAggregator = cwWpmAggregator;
        _selectedBandIndex = 5;  // 20 m
        _selectedModeIndex = 0;  // SSB
        _qsoStartTime = DateTimeOffset.UtcNow;

        _elapsedTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        _elapsedTimer.Tick += OnElapsedTimerTick;

        UpdateLogEnabled();
    }

    // ── Computed / read-only ─────────────────────────────────────────────

    public static BandOption[] BandOptions => OperatorOptions.Bands;
    public static ModeOption[] ModeOptions => OperatorOptions.Modes;

    public BandOption SelectedBand => OperatorOptions.Bands[SelectedBandIndex];
    public ModeOption SelectedMode => OperatorOptions.Modes[SelectedModeIndex];

    public string BandLabel => SelectedBand.Label;
    public string ModeLabel => SelectedMode.Label;
    internal CallsignRecord? LastLookupRecord => _lastLookupRecord;
    internal DateTimeOffset SuggestedUtcStart => _timerRunning ? _qsoStartTime : DateTimeOffset.UtcNow;

    /// <summary>
    /// True while the operator has an in-progress QSO entry — i.e., the
    /// elapsed-time timer is running because a non-empty callsign was typed
    /// and not yet logged, cleared, or abandoned. Consumed by
    /// <c>MainWindowViewModel</c> to gate the cw-decoder subprocess
    /// lifecycle on actual operator activity rather than running it
    /// continuously.
    /// </summary>
    internal bool IsLoggerEpisodeActive => _timerRunning;

    /// <summary>
    /// True when the active operator-selected mode is CW. Consumed by
    /// <c>MainWindowViewModel</c> together with
    /// <see cref="IsLoggerEpisodeActive"/> to gate the cw-decoder
    /// subprocess on actual CW QSOs — there's no value in hunting for
    /// keying on an SSB or FT8 contact and a stale lock from CW spillover
    /// would just noise up the WPM badge.
    /// </summary>
    internal bool IsLoggerOnCwMode => SelectedMode.ProtoMode == Mode.Cw;

    /// <summary>
    /// Raised when <see cref="IsLoggerOnCwMode"/> changes — i.e. the
    /// operator selected a different mode from the picker. The host uses
    /// this to start/stop the cw-decoder mid-episode without waiting for
    /// the current QSO to end. Fires before
    /// <see cref="ObservableObject.PropertyChanged"/> for SelectedMode.
    /// </summary>
    public event EventHandler? CwModeChanged;

    // ── Events ───────────────────────────────────────────────────────────

    /// <summary>Raised after a QSO is successfully logged.</summary>
    public event EventHandler? QsoLogged;

    /// <summary>
    /// Raised when a logged or cleared QSO crosses an episode boundary
    /// (whichever the diagnostics recorder, if any, should finalize against).
    /// The host (<see cref="MainWindowViewModel"/>) subscribes and writes
    /// the per-episode comparison snapshot. Fired on the same thread as the
    /// triggering action — UI thread for save success and Clear.
    /// </summary>
    public event EventHandler<CwEpisodeBoundaryEventArgs>? CwEpisodeBoundary;

    /// <summary>
    /// Raised when the operator first commits to a new QSO by typing into
    /// the Callsign field (the same trigger that starts the elapsed timer).
    /// The host (<see cref="MainWindowViewModel"/>) subscribes and asks an
    /// active <see cref="Services.CwDiagnosticsRecorder"/> to open a new
    /// per-QSO episode aligned to <c>UtcStart</c>. Idempotent semantics —
    /// the timer only starts once per QSO so this fires once per QSO.
    /// </summary>
    public event EventHandler<CwEpisodeStartedEventArgs>? CwEpisodeStarted;

    /// <summary>Raised when the view should move focus to the callsign field.</summary>
    public event EventHandler? LoggerFocusRequested;

    // ── Property-change hooks ────────────────────────────────────────────

    partial void OnCallsignChanged(string value)
    {
        var normalized = NormalizeCallsignInput(value);
        if (!string.Equals(value, normalized, StringComparison.Ordinal))
        {
            Callsign = normalized;
            return;
        }

        UpdateLogEnabled();

        // Update timer hint text
        if (!string.IsNullOrWhiteSpace(value) && !_timerRunning)
        {
            ElapsedTimeText = "Press F7";
        }
        else if (string.IsNullOrWhiteSpace(value))
        {
            // Operator backed out of a QSO without saving or pressing Clear.
            // Treat that the same as Clear() for episode-boundary purposes
            // so a diagnostics recorder doesn't leave the episode hanging.
            var boundaryStart = _qsoStartTime;
            var boundaryEnd = DateTimeOffset.UtcNow;
            StopTimer();
            ElapsedTimeText = "---";
            CwEpisodeBoundary?.Invoke(this, new CwEpisodeBoundaryEventArgs(
                Reason: "abandoned",
                Qso: null,
                UtcStart: boundaryStart,
                UtcEnd: boundaryEnd));
        }

        // Cancel any pending lookup
        _lookupCts?.Cancel();

        if (string.IsNullOrWhiteSpace(value) || value.Trim().Length < 3)
        {
            ClearLookupFields();
            return;
        }

        // Debounced lookup
        _lookupCts = new CancellationTokenSource();
        _ = DebouncedLookupAsync(value.Trim(), _lookupCts.Token);
    }

    partial void OnSelectedBandIndexChanged(int value)
    {
        if (value < 0 || value >= OperatorOptions.Bands.Length)
            return;

        OnPropertyChanged(nameof(SelectedBand));
        OnPropertyChanged(nameof(BandLabel));

        if (!_frequencyManuallySet)
        {
            FrequencyMhz = OperatorOptions.Bands[value].DefaultFrequencyMhz
                .ToString("F3", CultureInfo.InvariantCulture);
        }
    }

    partial void OnSelectedModeIndexChanged(int oldValue, int newValue)
    {
        if (newValue < 0 || newValue >= OperatorOptions.Modes.Length)
            return;

        OnPropertyChanged(nameof(SelectedMode));
        OnPropertyChanged(nameof(ModeLabel));
        OnPropertyChanged(nameof(IsLoggerOnCwMode));

        if (!_rstManuallySet)
        {
            var defaultRst = OperatorOptions.Modes[newValue].DefaultRst;
            RstSent = defaultRst;
            RstRcvd = defaultRst;
        }

        // Fire CwModeChanged whenever the CW-vs-not-CW classification
        // flips so the host can start/stop the cw-decoder subprocess
        // without waiting for the next episode boundary.
        var oldIsCw = oldValue >= 0 && oldValue < OperatorOptions.Modes.Length
            && OperatorOptions.Modes[oldValue].ProtoMode == Mode.Cw;
        var newIsCw = OperatorOptions.Modes[newValue].ProtoMode == Mode.Cw;
        if (oldIsCw != newIsCw)
        {
            CwModeChanged?.Invoke(this, EventArgs.Empty);
        }
    }

    // ── Band / mode cycling commands ─────────────────────────────────────

    [RelayCommand]
    private void CycleBandForward()
    {
        _bandManuallySet = true;
        SelectedBandIndex = (SelectedBandIndex + 1) % OperatorOptions.Bands.Length;
    }

    [RelayCommand]
    private void CycleBandBackward()
    {
        _bandManuallySet = true;
        SelectedBandIndex = (SelectedBandIndex - 1 + OperatorOptions.Bands.Length) % OperatorOptions.Bands.Length;
    }

    [RelayCommand]
    private void CycleModeForward()
    {
        _modeManuallySet = true;
        SelectedModeIndex = (SelectedModeIndex + 1) % OperatorOptions.Modes.Length;
    }

    [RelayCommand]
    private void CycleModeBackward()
    {
        _modeManuallySet = true;
        SelectedModeIndex = (SelectedModeIndex - 1 + OperatorOptions.Modes.Length) % OperatorOptions.Modes.Length;
    }

    // ── Log QSO command ──────────────────────────────────────────────────

    [RelayCommand]
    private async Task LogQsoAsync()
    {
        var callsign = Callsign.Trim().ToUpperInvariant();
        if (string.IsNullOrWhiteSpace(callsign))
        {
            return;
        }

        var band = SelectedBand;
        var mode = SelectedMode;
        var utcNow = DateTimeOffset.UtcNow;
        var utcStart = _timerRunning ? _qsoStartTime : utcNow;
        // Episode boundary / CW enrichment window always uses real "now" as
        // the end. UtcEndTimestamp on the QSO itself is gated on F7 below.
        var utcEnd = utcNow;

        var qso = new QsoRecord
        {
            WorkedCallsign = callsign,
            Band = band.ProtoBand,
            Mode = mode.ProtoMode,
            RstSent = ParseRst(RstSent.Trim()),
            RstReceived = ParseRst(RstRcvd.Trim()),
            UtcTimestamp = Timestamp.FromDateTimeOffset(utcStart),
        };

        // Only set end timestamp if timer was explicitly started via F7
        if (_timerRunning)
        {
            qso.UtcEndTimestamp = Timestamp.FromDateTimeOffset(utcNow);
        }

        if (!string.IsNullOrWhiteSpace(mode.Submode))
        {
            qso.Submode = mode.Submode;
        }

        if (double.TryParse(FrequencyMhz, NumberStyles.Float, CultureInfo.InvariantCulture, out var freqMhz)
            && freqMhz > 0)
        {
            var hz = (ulong)Math.Round(freqMhz * 1_000_000.0, MidpointRounding.AwayFromZero);
            qso.FrequencyHz = hz;
#pragma warning disable CS0612
            qso.FrequencyKhz = (hz + 500) / 1000;
#pragma warning restore CS0612
        }

        if (!string.IsNullOrWhiteSpace(Comment))
        {
            qso.Comment = Comment.Trim();
        }

        if (!string.IsNullOrWhiteSpace(Notes))
        {
            qso.Notes = Notes.Trim();
        }

        if (!string.IsNullOrWhiteSpace(ContestId))
        {
            qso.ContestId = ContestId.Trim();
        }

        if (!string.IsNullOrWhiteSpace(ExchangeSent))
        {
            qso.ExchangeSent = ExchangeSent.Trim();
        }

        EnrichFromLookup(qso, _lastLookupRecord);
        EnrichFromCwDecoder(qso, utcStart, utcEnd);

        LogStatusText = "Logging\u2026";
        IsLogEnabled = false;

        try
        {
            var response = await _engine.LogQsoAsync(qso);
            LogStatusText = $"Logged {callsign}";
            // Snapshot the window before Clear() resets _qsoStartTime, then
            // raise the boundary event so a host-side diagnostics recorder
            // (if attached) can finalize the episode against the actual QSO.
            var boundaryStart = utcStart;
            var boundaryEnd = utcEnd;
            Clear();
            QsoLogged?.Invoke(this, EventArgs.Empty);
            CwEpisodeBoundary?.Invoke(this, new CwEpisodeBoundaryEventArgs(
                Reason: "logged",
                Qso: qso,
                UtcStart: boundaryStart,
                UtcEnd: boundaryEnd));
            FocusLogger();
        }
        catch (Grpc.Core.RpcException ex)
        {
            LogStatusText = $"Error: {ex.Status.Detail}";
            IsLogEnabled = true;
        }
    }

    /// <summary>
    /// Copies cached callsign-lookup fields into the QSO record so the logged
    /// contact includes operator name, grid, country, DXCC, and zone data.
    /// </summary>
    /// <summary>
    /// Attach (or replace) the CW WPM aggregator used to populate
    /// <see cref="QsoRecord.CwDecodeRxWpm"/> on logged CW QSOs. The
    /// aggregator is owned by the host (typically the MainWindowViewModel)
    /// and may be null when CW decoding is disabled or unsupported.
    /// </summary>
    internal void AttachCwAggregator(CwQsoWpmAggregator? aggregator)
        => _cwWpmAggregator = aggregator;

    /// <summary>
    /// Attach (or replace) the CW transcript aggregator used to populate
    /// <see cref="QsoRecord.CwDecodeTranscript"/> on logged CW QSOs. The
    /// aggregator is owned by the host (typically the MainWindowViewModel)
    /// and may be null when CW decoding is disabled or unsupported.
    /// </summary>
    internal void AttachCwTranscriptAggregator(CwQsoTranscriptAggregator? aggregator)
        => _cwTranscriptAggregator = aggregator;

    /// <summary>
    /// Attach (or replace) the handler invoked from <see cref="ResetTimer"/>
    /// (F7) to release the CW decoder's current pitch lock so the next QSO
    /// starts hunting fresh. The handler is owned by the host (typically
    /// MainWindowViewModel) and may be null when the decoder is disabled.
    /// </summary>
    internal void AttachCwResetLockHandler(Action? handler)
        => _cwResetLockHandler = handler;

    /// <summary>
    /// Auto-fills <see cref="QsoRecord.CwDecodeRxWpm"/> and
    /// <see cref="QsoRecord.CwDecodeTranscript"/> from the live CW
    /// decoder when the QSO is on CW mode and the aggregator(s) have
    /// data inside the QSO's window. Non-CW QSOs and missing/empty
    /// sources are no-ops so logging is never blocked. Existing
    /// transcript text on the QSO (e.g. typed by the operator on the
    /// full card) is preserved — auto-fill never overwrites a
    /// caller-supplied transcript.
    /// </summary>
    internal void EnrichFromCwDecoder(QsoRecord qso, DateTimeOffset utcStart, DateTimeOffset utcEnd)
    {
        if (qso.Mode != Mode.Cw)
        {
            // Defensive: if mode isn't CW, scrub any auto-filled CW
            // fields so they can't ride along on a re-classified QSO.
            qso.ClearCwDecodeRxWpm();
            qso.ClearCwDecodeTranscript();
            return;
        }

        if (_cwWpmAggregator is not null)
        {
            var mean = _cwWpmAggregator.GetMeanWpm(utcStart, utcEnd);
            if (mean is not null && double.IsFinite(mean.Value) && mean.Value > 0)
            {
                var rounded = (uint)Math.Round(mean.Value, MidpointRounding.AwayFromZero);
                if (rounded > 0)
                {
                    qso.CwDecodeRxWpm = rounded;
                }
            }
        }

        // Operator wins: if the QSO already carries non-empty transcript
        // text (typed/edited on the full QSO card), don't overwrite it
        // with the decoder's snapshot.
        if (_cwTranscriptAggregator is not null
            && (!qso.HasCwDecodeTranscript || string.IsNullOrWhiteSpace(qso.CwDecodeTranscript)))
        {
            var transcript = _cwTranscriptAggregator.GetTranscript(utcStart, utcEnd);
            if (!string.IsNullOrWhiteSpace(transcript))
            {
                qso.CwDecodeTranscript = transcript;
            }
        }
    }

    internal static void EnrichFromLookup(QsoRecord qso, CallsignRecord? record)
    {
        if (record is not { } rec)
        {
            return;
        }

        var name = BuildName(rec.FirstName, rec.LastName);
        if (!string.IsNullOrEmpty(name))
        {
            qso.WorkedOperatorName = name;
        }

        if (!string.IsNullOrEmpty(rec.GridSquare))
        {
            qso.WorkedGrid = rec.GridSquare;
        }

        if (!string.IsNullOrEmpty(rec.Country))
        {
            qso.WorkedCountry = rec.Country;
        }

        if (rec.DxccEntityId != 0)
        {
            qso.WorkedDxcc = rec.DxccEntityId;
        }

        if (!string.IsNullOrEmpty(rec.State))
        {
            qso.WorkedState = rec.State;
        }

        if (rec.HasCqZone)
        {
            qso.WorkedCqZone = rec.CqZone;
        }

        if (rec.HasItuZone)
        {
            qso.WorkedItuZone = rec.ItuZone;
        }

        if (!string.IsNullOrEmpty(rec.County))
        {
            qso.WorkedCounty = rec.County;
        }

        if (!string.IsNullOrEmpty(rec.Iota))
        {
            qso.WorkedIota = rec.Iota;
        }

        if (!string.IsNullOrEmpty(rec.DxccContinent))
        {
            qso.WorkedContinent = rec.DxccContinent;
        }
    }

    // ── Clear / reset commands ───────────────────────────────────────────

    [RelayCommand]
    private void Clear()
    {
        // Snapshot the window before we reset state. We only fire the
        // episode boundary if the timer was running (i.e. there was an
        // operator-driven QSO attempt with a meaningful start time);
        // resetting an already-empty form is not an episode boundary.
        var hadTimer = _timerRunning;
        var boundaryStart = _qsoStartTime;
        var boundaryEnd = DateTimeOffset.UtcNow;

        _lookupCts?.Cancel();
        Callsign = string.Empty;
        Comment = string.Empty;
        Notes = string.Empty;
        ContestId = string.Empty;
        ExchangeSent = string.Empty;
        LogStatusText = string.Empty;
        ClearLookupFields();
        _frequencyManuallySet = false;
        _rstManuallySet = false;
        _bandManuallySet = false;
        _modeManuallySet = false;

        // Restore defaults — triggers OnSelectedBand/ModeIndexChanged which
        // will repopulate FrequencyMhz and RST from the default band/mode.
        SelectedBandIndex = 5;  // 20 m
        SelectedModeIndex = 0;  // SSB

        StopTimer();
        ElapsedTimeText = "---";
        UpdateLogEnabled();

        if (hadTimer)
        {
            CwEpisodeBoundary?.Invoke(this, new CwEpisodeBoundaryEventArgs(
                Reason: "cleared",
                Qso: null,
                UtcStart: boundaryStart,
                UtcEnd: boundaryEnd));
        }
    }

    [RelayCommand]
    private void AcknowledgeQsoStart()
    {
        _qsoStartTime = DateTimeOffset.UtcNow;
        _timerRunning = true;
        _elapsedTimer.Start();
        ElapsedTimeText = "00:00";
        // Fire the same boundary signal that the old auto-start path used,
        // so MainWindowViewModel (or any other host) can spin up the
        // cw-decoder subprocess on F7. Without this, IsLoggerEpisodeActive
        // would flip to true silently and downstream consumers would never
        // see the episode-started edge.
        CwEpisodeStarted?.Invoke(this, new CwEpisodeStartedEventArgs(_qsoStartTime));
        // F7 is the operator's "starting a new QSO" signal — also drop
        // any stale CW pitch lock from the previous contact so the
        // decoder re-acquires for the new station instead of bleeding
        // partial morse / WPM into the next QSO. Handler may be null
        // when the CW decoder is disabled or unavailable.
        try
        {
            _cwResetLockHandler?.Invoke();
        }
#pragma warning disable CA1031 // best-effort: never let a stuck child process block F7
        catch
        {
        }
#pragma warning restore CA1031
    }

    // ── Manual-override notifications ────────────────────────────────────
    // Called by the view when the user explicitly types in a field, so we
    // know not to overwrite that value on subsequent band/mode changes.

    public void NotifyFrequencyManuallySet()
    {
        _frequencyManuallySet = true;
    }

    public void NotifyRstManuallySet()
    {
        _rstManuallySet = true;
    }

    // ── Rig integration ──────────────────────────────────────────────────

    /// <summary>
    /// Apply a rig snapshot to untouched fields. Only fills band, mode and
    /// frequency when the callsign is empty (fresh/cleared form) and the
    /// field has not been manually overridden by the operator.
    /// </summary>
    public void ApplyRigSnapshot(RigSnapshot snapshot)
    {
        if (snapshot.Status != RigConnectionStatus.Connected)
        {
            return;
        }

        // Only auto-fill when callsign is empty (fresh form).
        if (!string.IsNullOrWhiteSpace(Callsign))
        {
            return;
        }

        if (!_bandManuallySet && snapshot.Band != Band.Unspecified)
        {
            SelectedBandIndex = OperatorOptions.FindBandIndex(snapshot.Band);
        }

        if (!_modeManuallySet && snapshot.Mode != Mode.Unspecified)
        {
            SelectedModeIndex = OperatorOptions.FindModeIndex(snapshot.Mode, snapshot.Submode);
        }

        if (!_frequencyManuallySet && snapshot.FrequencyHz > 0)
        {
            var mhz = snapshot.FrequencyHz / 1_000_000.0;
            FrequencyMhz = mhz.ToString("F3", CultureInfo.InvariantCulture);
        }
    }

    /// <summary>Request the view to focus the callsign entry field.</summary>
    public void FocusLogger()
    {
        LoggerFocusRequested?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>
    /// Accept a <see cref="CallsignRecord"/> resolved externally (e.g. from
    /// the F8 callsign card) so the next logged QSO includes enrichment data.
    /// Only applied when the callsign matches the current entry.
    /// </summary>
    public void AcceptLookupRecord(CallsignRecord record)
    {
        if (string.Equals(
                record.Callsign,
                Callsign.Trim(),
                StringComparison.OrdinalIgnoreCase))
        {
            _lastLookupRecord = record;
            LookupName = BuildName(record.FirstName, record.LastName);
            LookupGrid = record.GridSquare ?? string.Empty;
            LookupCountry = record.Country ?? string.Empty;
            LookupStatusText = string.Empty;
        }
    }

    // ── Debounced callsign lookup ───────────────────────────────────────

    private async Task DebouncedLookupAsync(string callsign, CancellationToken ct)
    {
        try
        {
            await Task.Delay(800, ct);
        }
        catch (TaskCanceledException)
        {
            return;
        }

        if (ct.IsCancellationRequested)
        {
            return;
        }

        LookupStatusText = "Looking up\u2026";

        try
        {
            var response = await _engine.LookupCallsignAsync(callsign, ct);
            if (ct.IsCancellationRequested)
            {
                return;
            }

            var result = response.Result;
            if (result is not null && result.State == LookupState.Found)
            {
                var record = result.Record;
                if (record is not null)
                {
                    _lastLookupRecord = record;
                    LookupName = BuildName(record.FirstName, record.LastName);
                    LookupGrid = record.GridSquare ?? string.Empty;
                    LookupCountry = record.Country ?? string.Empty;
                    LookupStatusText = string.Empty;
                }
                else
                {
                    LookupStatusText = "No data";
                }
            }
            else
            {
                ClearLookupFields();
                LookupStatusText = "Not found";
            }
        }
        catch (TaskCanceledException)
        {
            // Lookup was cancelled — expected when user keeps typing
        }
        catch (Grpc.Core.RpcException)
        {
            LookupStatusText = "Lookup error";
        }
    }

    private void ClearLookupFields()
    {
        _lastLookupRecord = null;
        LookupName = string.Empty;
        LookupGrid = string.Empty;
        LookupCountry = string.Empty;
        LookupStatusText = string.Empty;
    }

    private static string BuildName(string? first, string? last)
    {
        var parts = new List<string>(2);
        if (!string.IsNullOrWhiteSpace(first))
        {
            parts.Add(first.Trim());
        }

        if (!string.IsNullOrWhiteSpace(last))
        {
            parts.Add(last.Trim());
        }

        return string.Join(" ", parts);
    }

    private static string NormalizeCallsignInput(string value)
    {
        return string.IsNullOrEmpty(value)
            ? string.Empty
            : value.ToUpperInvariant();
    }

    // ── Timer helpers ────────────────────────────────────────────────────

    private void StartTimer()
    {
        _qsoStartTime = DateTimeOffset.UtcNow;
        _timerRunning = true;
        _elapsedTimer.Start();
        CwEpisodeStarted?.Invoke(this, new CwEpisodeStartedEventArgs(_qsoStartTime));
    }

    private void StopTimer()
    {
        _timerRunning = false;
        _elapsedTimer.Stop();
    }

    private void OnElapsedTimerTick(object? sender, EventArgs e)
    {
        var elapsed = DateTimeOffset.UtcNow - _qsoStartTime;
        ElapsedTimeText = elapsed.TotalHours >= 1
            ? elapsed.ToString(@"h\:mm\:ss", CultureInfo.InvariantCulture)
            : elapsed.ToString(@"mm\:ss", CultureInfo.InvariantCulture);
    }

    // ── Private helpers ──────────────────────────────────────────────────

    private void UpdateLogEnabled()
    {
        IsLogEnabled = !string.IsNullOrWhiteSpace(Callsign);
    }

    /// <summary>
    /// Parse an RST string (e.g. "59", "599") into a <see cref="RstReport"/>
    /// with the individual digit fields populated alongside the raw text.
    /// </summary>
    private static RstReport ParseRst(string value)
    {
        var report = new RstReport { Raw = value };

        if (value.Length is (2 or 3) && value.All(static c => char.IsAsciiDigit(c)))
        {
            report.Readability = (uint)(value[0] - '0');
            report.Strength = (uint)(value[1] - '0');

            if (value.Length == 3)
            {
                report.Tone = (uint)(value[2] - '0');
            }
        }

        return report;
    }
}
