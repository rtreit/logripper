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
    {
        _engine = engine;
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

    // ── Events ───────────────────────────────────────────────────────────

    /// <summary>Raised after a QSO is successfully logged.</summary>
    public event EventHandler? QsoLogged;

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

        if (!string.IsNullOrWhiteSpace(value) && !_timerRunning)
        {
            StartTimer();
        }
        else if (string.IsNullOrWhiteSpace(value) && _timerRunning)
        {
            StopTimer();
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

    partial void OnSelectedModeIndexChanged(int value)
    {
        if (value < 0 || value >= OperatorOptions.Modes.Length)
            return;

        OnPropertyChanged(nameof(SelectedMode));
        OnPropertyChanged(nameof(ModeLabel));

        if (!_rstManuallySet)
        {
            var defaultRst = OperatorOptions.Modes[value].DefaultRst;
            RstSent = defaultRst;
            RstRcvd = defaultRst;
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

        var qso = new QsoRecord
        {
            WorkedCallsign = callsign,
            Band = band.ProtoBand,
            Mode = mode.ProtoMode,
            RstSent = ParseRst(RstSent.Trim()),
            RstReceived = ParseRst(RstRcvd.Trim()),
            UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
        };

        if (!string.IsNullOrWhiteSpace(mode.Submode))
        {
            qso.Submode = mode.Submode;
        }

        if (double.TryParse(FrequencyMhz, NumberStyles.Float, CultureInfo.InvariantCulture, out var freqMhz)
            && freqMhz > 0)
        {
            qso.FrequencyKhz = (ulong)(freqMhz * 1000.0);
        }

        if (!string.IsNullOrWhiteSpace(Comment))
        {
            qso.Comment = Comment.Trim();
        }

        EnrichFromLookup(qso, _lastLookupRecord);

        LogStatusText = "Logging\u2026";
        IsLogEnabled = false;

        try
        {
            var response = await _engine.LogQsoAsync(qso);
            LogStatusText = $"Logged {callsign}";
            Clear();
            QsoLogged?.Invoke(this, EventArgs.Empty);
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
        _lookupCts?.Cancel();
        Callsign = string.Empty;
        Comment = string.Empty;
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
        ElapsedTimeText = "00:00";
        UpdateLogEnabled();
    }

    [RelayCommand]
    private void ResetTimer()
    {
        _qsoStartTime = DateTimeOffset.UtcNow;
        ElapsedTimeText = "00:00";
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
