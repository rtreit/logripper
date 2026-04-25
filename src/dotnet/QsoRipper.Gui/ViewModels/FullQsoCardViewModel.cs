using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Google.Protobuf.Collections;
using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class FullQsoCardViewModel : ObservableObject, IDisposable
{
    private static readonly string[] TimestampFormats =
    [
        "yyyy-MM-dd HH:mm",
        "yyyy-MM-dd HH:mm:ss",
        "yy-MM-dd HH:mm",
        "yyyy-MM-ddTHH:mm",
        "yyyy-MM-ddTHH:mm:ss",
        "yyyy-MM-ddTHH:mm:ssZ",
        "yyyy-MM-ddTHH:mm:ss.fffZ",
        "yyyy-MM-dd",
        "o"
    ];

    private static readonly string[] BandLabels = System.Enum.GetValues<Band>()
        .Where(static value => value != Band.Unspecified)
        .Select(ProtoEnumDisplay.ForBand)
        .ToArray();

    private static readonly string[] ModeLabels = System.Enum.GetValues<Mode>()
        .Where(static value => value != Mode.Unspecified)
        .Select(ProtoEnumDisplay.ForMode)
        .ToArray();

    private static readonly string[] QslStatusLabels = ["-", "No", "Yes", "Requested", "Queued", "Ignore"];
    private const string SectionHintText = "Ctrl+Tab / Ctrl+Shift+Tab or Alt+1..7 to switch sections";

    private readonly IEngineClient _engine;
    private readonly QsoLoggerViewModel? _logger;
    private readonly QsoRecord? _sourceQso;
    private string _lastAutoWorkedOperatorCallsign = string.Empty;
    private CancellationTokenSource? _lookupCts;
    private CallsignRecord? _lastLookupRecord;

    private FullQsoCardViewModel(IEngineClient engine, QsoLoggerViewModel? logger, QsoRecord? sourceQso, StationProfile? activeStationProfile)
    {
        _engine = engine;
        _logger = logger;
        _sourceQso = sourceQso?.Clone();

        if (_sourceQso is not null)
        {
            SeedFromQso(_sourceQso);
            InitializeStationTabState();
            return;
        }

        SeedFromLogger(logger ?? throw new ArgumentNullException(nameof(logger)));
        SeedFromStationProfile(activeStationProfile);
        InitializeStationTabState();
    }

    public static FullQsoCardViewModel ForNew(IEngineClient engine, QsoLoggerViewModel logger, StationProfile? activeStationProfile = null) =>
        new(engine, logger, null, activeStationProfile);

    public static FullQsoCardViewModel ForEdit(IEngineClient engine, QsoRecord qso) =>
        new(engine, null, qso, null);

    public IReadOnlyList<string> BandOptions { get; } = BandLabels;
    public IReadOnlyList<string> ModeOptions { get; } = ModeLabels;
    public IReadOnlyList<string> QslStatusOptions { get; } = QslStatusLabels;

    public bool IsEditingExisting => _sourceQso is not null;
    public string CardTitle => IsEditingExisting ? "QSO Card - Edit" : "QSO Card - New";
    public string CardSubtitle => IsEditingExisting
        ? "Editing the selected QSO. Ctrl+Enter saves, Esc closes."
        : "Advanced entry for a new QSO. Ctrl+Enter / F10 logs, Esc closes.";
    public string SaveButtonText => IsEditingExisting ? "Save Changes" : "Log QSO";
    public string ModeBadgeText => IsEditingExisting ? "Edit Existing" : "New Contact";
    public string SectionHint { get; } = SectionHintText;
    internal TimeSpan LookupDebounceDelay { get; set; } = TimeSpan.FromMilliseconds(800);

    [ObservableProperty]
    private int _selectedTabIndex;

    [ObservableProperty]
    private bool _isSaving;

    [ObservableProperty]
    private string _statusText = string.Empty;

    [ObservableProperty]
    private bool _isLookingUp;

    [ObservableProperty]
    private string _lookupStatusText = string.Empty;

    [ObservableProperty]
    private string _workedCallsign = string.Empty;

    [ObservableProperty]
    private string _stationCallsign = string.Empty;

    [ObservableProperty]
    private string _utcStartText = string.Empty;

    [ObservableProperty]
    private string _utcEndText = string.Empty;

    [ObservableProperty]
    private string _selectedBand = BandLabels.FirstOrDefault() ?? string.Empty;

    [ObservableProperty]
    private string _selectedMode = ModeLabels.FirstOrDefault() ?? string.Empty;

    [ObservableProperty]
    private string _submode = string.Empty;

    [ObservableProperty]
    private string _frequencyMhz = string.Empty;

    [ObservableProperty]
    private string _rstSent = string.Empty;

    [ObservableProperty]
    private string _rstReceived = string.Empty;

    [ObservableProperty]
    private string _txPower = string.Empty;

    [ObservableProperty]
    private string _workedOperatorCallsign = string.Empty;

    [ObservableProperty]
    private string _workedOperatorName = string.Empty;

    [ObservableProperty]
    private string _workedGrid = string.Empty;

    [ObservableProperty]
    private string _workedCountry = string.Empty;

    [ObservableProperty]
    private string _workedDxcc = string.Empty;

    [ObservableProperty]
    private string _workedState = string.Empty;

    [ObservableProperty]
    private string _workedCqZone = string.Empty;

    [ObservableProperty]
    private string _workedItuZone = string.Empty;

    [ObservableProperty]
    private string _workedCounty = string.Empty;

    [ObservableProperty]
    private string _workedIota = string.Empty;

    [ObservableProperty]
    private string _workedContinent = string.Empty;

    [ObservableProperty]
    private string _workedArrlSection = string.Empty;

    [ObservableProperty]
    private string _skcc = string.Empty;

    [ObservableProperty]
    private string _selectedQslSentStatus = QslStatusLabels[0];

    [ObservableProperty]
    private string _selectedQslReceivedStatus = QslStatusLabels[0];

    [ObservableProperty]
    private bool? _lotwSent;

    [ObservableProperty]
    private bool? _lotwReceived;

    [ObservableProperty]
    private bool? _eqslSent;

    [ObservableProperty]
    private bool? _eqslReceived;

    [ObservableProperty]
    private string _qslSentDateText = string.Empty;

    [ObservableProperty]
    private string _qslReceivedDateText = string.Empty;

    [ObservableProperty]
    private string _qrzLogId = string.Empty;

    [ObservableProperty]
    private string _qrzBookId = string.Empty;

    [ObservableProperty]
    private string _contestId = string.Empty;

    [ObservableProperty]
    private string _serialSent = string.Empty;

    [ObservableProperty]
    private string _serialReceived = string.Empty;

    [ObservableProperty]
    private string _exchangeSent = string.Empty;

    [ObservableProperty]
    private string _exchangeReceived = string.Empty;

    [ObservableProperty]
    private string _propMode = string.Empty;

    [ObservableProperty]
    private string _satName = string.Empty;

    [ObservableProperty]
    private string _satMode = string.Empty;

    [ObservableProperty]
    private string _notes = string.Empty;

    [ObservableProperty]
    private string _comment = string.Empty;

    [ObservableProperty]
    private string _cwDecodeRxWpmText = string.Empty;

    [ObservableProperty]
    private string _cwDecodeTranscript = string.Empty;

    [ObservableProperty]
    private string _snapshotProfileName = string.Empty;

    [ObservableProperty]
    private string _snapshotStationCallsign = string.Empty;

    [ObservableProperty]
    private string _snapshotOperatorCallsign = string.Empty;

    [ObservableProperty]
    private string _snapshotOperatorName = string.Empty;

    [ObservableProperty]
    private string _snapshotGrid = string.Empty;

    [ObservableProperty]
    private string _snapshotCounty = string.Empty;

    [ObservableProperty]
    private string _snapshotState = string.Empty;

    [ObservableProperty]
    private string _snapshotCountry = string.Empty;

    [ObservableProperty]
    private string _snapshotDxcc = string.Empty;

    [ObservableProperty]
    private string _snapshotCqZone = string.Empty;

    [ObservableProperty]
    private string _snapshotItuZone = string.Empty;

    [ObservableProperty]
    private string _snapshotLatitude = string.Empty;

    [ObservableProperty]
    private string _snapshotLongitude = string.Empty;

    [ObservableProperty]
    private string _snapshotArrlSection = string.Empty;

    [ObservableProperty]
    private bool _showAdvancedStationFields;

    [ObservableProperty]
    private string _localId = string.Empty;

    [ObservableProperty]
    private string _syncStatusText = "Local";

    [ObservableProperty]
    private string _createdAtText = string.Empty;

    [ObservableProperty]
    private string _updatedAtText = string.Empty;

    [ObservableProperty]
    private string _extraFieldsText = string.Empty;

    public event EventHandler? CloseRequested;
    public event EventHandler? Saved;

    partial void OnWorkedCallsignChanged(string value)
    {
        var normalized = NormalizeToken(value, uppercase: true);
        if (!string.Equals(value, normalized, StringComparison.Ordinal))
        {
            WorkedCallsign = normalized;
            return;
        }

        if (string.IsNullOrWhiteSpace(WorkedOperatorCallsign)
            || string.Equals(WorkedOperatorCallsign, _lastAutoWorkedOperatorCallsign, StringComparison.Ordinal))
        {
            WorkedOperatorCallsign = normalized;
            _lastAutoWorkedOperatorCallsign = normalized;
        }

        RestartLookupForWorkedCallsign(normalized);
    }

    partial void OnWorkedOperatorCallsignChanged(string value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            _lastAutoWorkedOperatorCallsign = string.Empty;
        }
    }

    partial void OnCwDecodeRxWpmTextChanged(string value)
    {
        OnPropertyChanged(nameof(CwTranscriptSummary));
        OnPropertyChanged(nameof(CwTranscriptSourceBadge));
    }

    partial void OnCwDecodeTranscriptChanged(string value)
    {
        OnPropertyChanged(nameof(CwTranscriptSummary));
        OnPropertyChanged(nameof(CwTranscriptPreview));
        OnPropertyChanged(nameof(HasCwTranscriptContent));
    }

    /// <summary>
    /// True when CW WPM or transcript is populated. Used to show a "no data" state in the
    /// Transcript tab without hiding the editable fields.
    /// </summary>
    public bool HasCwTranscriptContent =>
        !string.IsNullOrWhiteSpace(CwDecodeTranscript) || !string.IsNullOrWhiteSpace(CwDecodeRxWpmText);

    /// <summary>
    /// Compact summary line shown on the Core tab so the operator can see at-a-glance whether
    /// CW decoder data is captured without leaving the section.
    /// </summary>
    public string CwTranscriptSummary
    {
        get
        {
            var hasWpm = !string.IsNullOrWhiteSpace(CwDecodeRxWpmText);
            var hasText = !string.IsNullOrWhiteSpace(CwDecodeTranscript);
            if (!hasWpm && !hasText)
            {
                return "No CW decoder data captured.";
            }

            var parts = new List<string>(2);
            if (hasWpm)
            {
                parts.Add(string.Concat(CwDecodeRxWpmText.Trim(), " WPM"));
            }

            if (hasText)
            {
                parts.Add(string.Concat(
                    CwDecodeTranscript.Length.ToString(CultureInfo.InvariantCulture),
                    " chars"));
            }

            return string.Join(" \u00B7 ", parts);
        }
    }

    /// <summary>
    /// First ~80 characters of the CW transcript with newlines collapsed, for the Core tab preview.
    /// </summary>
    public string CwTranscriptPreview
    {
        get
        {
            if (string.IsNullOrWhiteSpace(CwDecodeTranscript))
            {
                return string.Empty;
            }

            var collapsed = CwDecodeTranscript.Replace('\n', ' ').Replace('\r', ' ').Trim();
            return collapsed.Length <= 80 ? collapsed : string.Concat(collapsed.AsSpan(0, 79), "\u2026");
        }
    }

    /// <summary>
    /// Source-and-metric badge shown atop each transcript section in the Transcript tab.
    /// Designed to be reused for future voice STT sections (e.g. "Voice STT \u00B7 Whisper").
    /// </summary>
    public string CwTranscriptSourceBadge =>
        string.IsNullOrWhiteSpace(CwDecodeRxWpmText)
            ? "CW decoder (RX)"
            : string.Concat("CW decoder (RX) \u00B7 ", CwDecodeRxWpmText.Trim(), " WPM");

    [RelayCommand]
    private void ShowTranscriptTab()
    {
        SelectedTabIndex = 5;
    }

    [RelayCommand]
    private void Close()
    {
        DisposeLookupCts();
        CloseRequested?.Invoke(this, EventArgs.Empty);
    }

    [RelayCommand]
    private async Task SaveAsync()
    {
        if (IsSaving)
        {
            return;
        }

        if (!TryBuildQso(out var qso, out var error))
        {
            StatusText = error ?? "Unable to build QSO.";
            return;
        }

        IsSaving = true;
        StatusText = IsEditingExisting ? "Saving changes..." : "Logging QSO...";

        try
        {
            if (IsEditingExisting)
            {
                await _engine.UpdateQsoAsync(qso);
                StatusText = $"Updated {qso.WorkedCallsign}.";
            }
            else
            {
                // Auto-fill CW WPM/transcript from the live decoder if the
                // operator hasn't typed values manually. Mirrors the
                // simple-logger path so QSOs logged via the full card
                // get the same CW enrichment.
                var cwStart = qso.UtcTimestamp?.ToDateTimeOffset() ?? DateTimeOffset.UtcNow;
                var cwEnd = qso.UtcEndTimestamp?.ToDateTimeOffset() ?? DateTimeOffset.UtcNow;
                _logger?.EnrichFromCwDecoder(qso, cwStart, cwEnd);

                var response = await _engine.LogQsoAsync(qso);
                LocalId = response.LocalId;
                StatusText = $"Logged {qso.WorkedCallsign}.";

                if (_logger is not null)
                {
                    _logger.ClearCommand.Execute(null);
                    _logger.LogStatusText = StatusText;
                }
            }

            Saved?.Invoke(this, EventArgs.Empty);
            CloseRequested?.Invoke(this, EventArgs.Empty);
        }
        catch (RpcException ex)
        {
            StatusText = $"Error: {ex.Status.Detail}";
        }
        finally
        {
            IsSaving = false;
        }
    }

    [RelayCommand]
    private async Task LookupWorkedCallsignAsync()
    {
        if (IsLookingUp)
        {
            return;
        }

        var lookupCallsign = NormalizeToken(
            FirstNonBlank(WorkedCallsign, WorkedOperatorCallsign),
            uppercase: true);
        if (lookupCallsign.Length == 0)
        {
            LookupStatusText = "Enter a callsign to look up.";
            return;
        }

        PrepareForLookup(lookupCallsign);
        IsLookingUp = true;
        LookupStatusText = $"Looking up {lookupCallsign}...";

        try
        {
            LookupStatusText = await ExecuteLookupAsync(lookupCallsign, updateStatusText: true, CancellationToken.None);
        }
        catch (RpcException ex)
        {
            LookupStatusText = $"Lookup error: {ex.Status.Detail}";
        }
        catch (InvalidOperationException ex)
        {
            LookupStatusText = $"Lookup error: {ex.Message}";
        }
        finally
        {
            IsLookingUp = false;
        }
    }

    private void SeedFromLogger(QsoLoggerViewModel logger)
    {
        WorkedCallsign = logger.Callsign.Trim().ToUpperInvariant();
        WorkedOperatorCallsign = WorkedCallsign;
        _lastAutoWorkedOperatorCallsign = WorkedOperatorCallsign;
        SelectedBand = ProtoEnumDisplay.ForBand(logger.SelectedBand.ProtoBand);
        SelectedMode = ProtoEnumDisplay.ForMode(logger.SelectedMode.ProtoMode);
        Submode = logger.SelectedMode.Submode ?? string.Empty;
        FrequencyMhz = logger.FrequencyMhz;
        RstSent = logger.RstSent;
        RstReceived = logger.RstRcvd;
        ContestId = logger.ContestId;
        ExchangeSent = logger.ExchangeSent;
        Comment = logger.Comment;
        Notes = logger.Notes;
        UtcStartText = FormatTimestamp(logger.SuggestedUtcStart);
        UtcEndText = FormatTimestamp(DateTimeOffset.UtcNow);
        SyncStatusText = BuildSyncStatus(SyncStatus.LocalOnly);

        if (logger.LastLookupRecord is { } record)
        {
            ApplyLookup(record);
        }
    }

    private void SeedFromStationProfile(StationProfile? profile)
    {
        if (profile is null)
        {
            return;
        }

        StationCallsign = FirstNonBlank(StationCallsign, profile.StationCallsign) ?? string.Empty;
        SnapshotProfileName = FirstNonBlank(SnapshotProfileName, profile.ProfileName) ?? string.Empty;
        SnapshotStationCallsign = FirstNonBlank(SnapshotStationCallsign, profile.StationCallsign) ?? string.Empty;
        SnapshotOperatorCallsign = FirstNonBlank(SnapshotOperatorCallsign, profile.OperatorCallsign) ?? string.Empty;
        SnapshotOperatorName = FirstNonBlank(SnapshotOperatorName, profile.OperatorName) ?? string.Empty;
        SnapshotGrid = FirstNonBlank(SnapshotGrid, profile.Grid) ?? string.Empty;
        SnapshotCounty = FirstNonBlank(SnapshotCounty, profile.County) ?? string.Empty;
        SnapshotState = FirstNonBlank(SnapshotState, profile.State) ?? string.Empty;
        SnapshotCountry = FirstNonBlank(SnapshotCountry, profile.Country) ?? string.Empty;
        SnapshotArrlSection = FirstNonBlank(SnapshotArrlSection, profile.ArrlSection) ?? string.Empty;

        if (string.IsNullOrWhiteSpace(SnapshotDxcc) && profile.HasDxcc)
        {
            SnapshotDxcc = profile.Dxcc.ToString(CultureInfo.InvariantCulture);
        }

        if (string.IsNullOrWhiteSpace(SnapshotCqZone) && profile.HasCqZone)
        {
            SnapshotCqZone = profile.CqZone.ToString(CultureInfo.InvariantCulture);
        }

        if (string.IsNullOrWhiteSpace(SnapshotItuZone) && profile.HasItuZone)
        {
            SnapshotItuZone = profile.ItuZone.ToString(CultureInfo.InvariantCulture);
        }

        if (string.IsNullOrWhiteSpace(SnapshotLatitude) && profile.HasLatitude)
        {
            SnapshotLatitude = profile.Latitude.ToString("0.####", CultureInfo.InvariantCulture);
        }

        if (string.IsNullOrWhiteSpace(SnapshotLongitude) && profile.HasLongitude)
        {
            SnapshotLongitude = profile.Longitude.ToString("0.####", CultureInfo.InvariantCulture);
        }
    }

    private void SeedFromQso(QsoRecord qso)
    {
        WorkedCallsign = qso.WorkedCallsign;
        StationCallsign = qso.StationCallsign;
        UtcStartText = FormatTimestamp(qso.UtcTimestamp);
        UtcEndText = FormatTimestamp(qso.UtcEndTimestamp);
        SelectedBand = ProtoEnumDisplay.ForBand(qso.Band);
        SelectedMode = ProtoEnumDisplay.ForMode(qso.Mode);
        Submode = qso.Submode ?? string.Empty;
        FrequencyMhz = qso.HasFrequencyHz ? FormatFrequencyMhz(qso.FrequencyHz)
#pragma warning disable CS0612
            : qso.HasFrequencyKhz ? FormatFrequencyMhz(qso.FrequencyKhz * 1000)
#pragma warning restore CS0612
            : string.Empty;
        RstSent = FormatRst(qso.RstSent);
        RstReceived = FormatRst(qso.RstReceived);
        TxPower = qso.TxPower ?? string.Empty;
        WorkedOperatorCallsign = FirstNonBlank(qso.WorkedOperatorCallsign, qso.WorkedCallsign) ?? string.Empty;
        _lastAutoWorkedOperatorCallsign = WorkedOperatorCallsign;
        WorkedOperatorName = qso.WorkedOperatorName ?? string.Empty;
        WorkedGrid = qso.WorkedGrid ?? string.Empty;
        WorkedCountry = qso.WorkedCountry ?? string.Empty;
        WorkedDxcc = qso.HasWorkedDxcc ? qso.WorkedDxcc.ToString(CultureInfo.InvariantCulture) : string.Empty;
        WorkedState = qso.WorkedState ?? string.Empty;
        WorkedCqZone = qso.HasWorkedCqZone ? qso.WorkedCqZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
        WorkedItuZone = qso.HasWorkedItuZone ? qso.WorkedItuZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
        WorkedCounty = qso.WorkedCounty ?? string.Empty;
        WorkedIota = qso.WorkedIota ?? string.Empty;
        WorkedContinent = qso.WorkedContinent ?? string.Empty;
        WorkedArrlSection = qso.WorkedArrlSection ?? string.Empty;
        Skcc = qso.Skcc ?? string.Empty;
        SelectedQslSentStatus = FormatQslStatus(qso.QslSentStatus);
        SelectedQslReceivedStatus = FormatQslStatus(qso.QslReceivedStatus);
        LotwSent = qso.HasLotwSent ? qso.LotwSent : null;
        LotwReceived = qso.HasLotwReceived ? qso.LotwReceived : null;
        EqslSent = qso.HasEqslSent ? qso.EqslSent : null;
        EqslReceived = qso.HasEqslReceived ? qso.EqslReceived : null;
        QslSentDateText = FormatDate(qso.QslSentDate);
        QslReceivedDateText = FormatDate(qso.QslReceivedDate);
        QrzLogId = qso.QrzLogid ?? string.Empty;
        QrzBookId = qso.QrzBookid ?? string.Empty;
        ContestId = qso.ContestId ?? string.Empty;
        SerialSent = qso.SerialSent ?? string.Empty;
        SerialReceived = qso.SerialReceived ?? string.Empty;
        ExchangeSent = qso.ExchangeSent ?? string.Empty;
        ExchangeReceived = qso.ExchangeReceived ?? string.Empty;
        PropMode = qso.PropMode ?? string.Empty;
        SatName = qso.SatName ?? string.Empty;
        SatMode = qso.SatMode ?? string.Empty;
        Notes = qso.Notes ?? string.Empty;
        Comment = qso.Comment ?? string.Empty;
        CwDecodeRxWpmText = qso.HasCwDecodeRxWpm
            ? qso.CwDecodeRxWpm.ToString(CultureInfo.InvariantCulture)
            : string.Empty;
        CwDecodeTranscript = qso.HasCwDecodeTranscript ? (qso.CwDecodeTranscript ?? string.Empty) : string.Empty;
        LocalId = qso.LocalId;
        SyncStatusText = BuildSyncStatus(qso.SyncStatus);
        CreatedAtText = FormatTimestamp(qso.CreatedAt);
        UpdatedAtText = FormatTimestamp(qso.UpdatedAt);
        ExtraFieldsText = FormatExtraFields(qso.ExtraFields);

        if (qso.StationSnapshot is { } snapshot)
        {
            SnapshotProfileName = snapshot.ProfileName ?? string.Empty;
            SnapshotStationCallsign = snapshot.StationCallsign;
            SnapshotOperatorCallsign = snapshot.OperatorCallsign ?? string.Empty;
            SnapshotOperatorName = snapshot.OperatorName ?? string.Empty;
            SnapshotGrid = snapshot.Grid ?? string.Empty;
            SnapshotCounty = snapshot.County ?? string.Empty;
            SnapshotState = snapshot.State ?? string.Empty;
            SnapshotCountry = snapshot.Country ?? string.Empty;
            SnapshotDxcc = snapshot.HasDxcc ? snapshot.Dxcc.ToString(CultureInfo.InvariantCulture) : string.Empty;
            SnapshotCqZone = snapshot.HasCqZone ? snapshot.CqZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
            SnapshotItuZone = snapshot.HasItuZone ? snapshot.ItuZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
            SnapshotLatitude = snapshot.HasLatitude ? snapshot.Latitude.ToString("0.####", CultureInfo.InvariantCulture) : string.Empty;
            SnapshotLongitude = snapshot.HasLongitude ? snapshot.Longitude.ToString("0.####", CultureInfo.InvariantCulture) : string.Empty;
            SnapshotArrlSection = snapshot.ArrlSection ?? string.Empty;
        }
    }

    private void ApplyLookup(CallsignRecord record)
    {
        _lastLookupRecord = record;
        WorkedOperatorCallsign = FirstNonBlank(WorkedOperatorCallsign, record.Callsign, record.CrossRef, WorkedCallsign) ?? string.Empty;
        _lastAutoWorkedOperatorCallsign = WorkedOperatorCallsign;
        WorkedOperatorName = FirstNonBlank(WorkedOperatorName, BuildName(record)) ?? string.Empty;
        WorkedGrid = FirstNonBlank(WorkedGrid, record.GridSquare) ?? string.Empty;
        WorkedCountry = FirstNonBlank(WorkedCountry, record.DxccCountryName, record.Country) ?? string.Empty;
        WorkedState = FirstNonBlank(WorkedState, record.State) ?? string.Empty;
        WorkedCounty = FirstNonBlank(WorkedCounty, record.County) ?? string.Empty;
        WorkedContinent = FirstNonBlank(WorkedContinent, record.DxccContinent) ?? string.Empty;
        WorkedIota = FirstNonBlank(WorkedIota, record.Iota) ?? string.Empty;

        if (string.IsNullOrWhiteSpace(WorkedDxcc) && record.DxccEntityId != 0)
        {
            WorkedDxcc = record.DxccEntityId.ToString(CultureInfo.InvariantCulture);
        }

        if (string.IsNullOrWhiteSpace(WorkedCqZone) && record.HasCqZone)
        {
            WorkedCqZone = record.CqZone.ToString(CultureInfo.InvariantCulture);
        }

        if (string.IsNullOrWhiteSpace(WorkedItuZone) && record.HasItuZone)
        {
            WorkedItuZone = record.ItuZone.ToString(CultureInfo.InvariantCulture);
        }
    }

    private static string BuildLookupStatusText(string lookupCallsign, LookupResult result)
    {
        var cacheSuffix = result.CacheHit ? " (cached)" : string.Empty;
        return result.LookupLatencyMs > 0
            ? $"Loaded {lookupCallsign} in {result.LookupLatencyMs} ms{cacheSuffix}."
            : $"Loaded {lookupCallsign}{cacheSuffix}.";
    }

    private void RestartLookupForWorkedCallsign(string workedCallsign)
    {
        DisposeLookupCts();

        if (workedCallsign.Length < 3)
        {
            ClearAppliedLookupFields();
            LookupStatusText = string.Empty;
            return;
        }

        _lookupCts = new CancellationTokenSource();
        _ = DebouncedLookupAsync(workedCallsign, _lookupCts.Token);
    }

    private async Task DebouncedLookupAsync(string callsign, CancellationToken ct)
    {
        try
        {
            await Task.Delay(LookupDebounceDelay, ct);
        }
        catch (TaskCanceledException)
        {
            return;
        }

        if (ct.IsCancellationRequested)
        {
            return;
        }

        PrepareForLookup(callsign);
        LookupStatusText = "Looking up...";

        try
        {
            LookupStatusText = await ExecuteLookupAsync(callsign, updateStatusText: false, ct);
        }
        catch (TaskCanceledException)
        {
        }
        catch (RpcException)
        {
            LookupStatusText = "Lookup error";
        }
        catch (InvalidOperationException)
        {
            LookupStatusText = "Lookup error";
        }
    }

    private async Task<string> ExecuteLookupAsync(string lookupCallsign, bool updateStatusText, CancellationToken ct)
    {
        var response = await _engine.LookupCallsignAsync(lookupCallsign, ct);
        if (ct.IsCancellationRequested)
        {
            return LookupStatusText;
        }

        var result = response.Result;
        if (result.State == LookupState.Found && result.Record is { } record)
        {
            ApplyLookup(record);
            return updateStatusText ? BuildLookupStatusText(lookupCallsign, result) : string.Empty;
        }

        ClearAppliedLookupFields();
        if (result.State == LookupState.NotFound)
        {
            return updateStatusText ? $"Callsign '{lookupCallsign}' not found." : "Not found";
        }

        return result.ErrorMessage ?? $"Lookup failed ({result.State}).";
    }

    private void PrepareForLookup(string lookupCallsign)
    {
        if (_lastLookupRecord is null)
        {
            return;
        }

        var previousCallsign = FirstNonBlank(
            _lastLookupRecord.Callsign,
            _lastLookupRecord.CrossRef,
            _lastLookupRecord.BaseCallsign);
        if (string.Equals(previousCallsign, lookupCallsign, StringComparison.OrdinalIgnoreCase))
        {
            return;
        }

        ClearAppliedLookupFields();
    }

    private void ClearAppliedLookupFields()
    {
        if (_lastLookupRecord is not { } record)
        {
            return;
        }

        ClearAutoField(WorkedOperatorName, BuildName(record), value => WorkedOperatorName = value);
        ClearAutoField(WorkedGrid, record.GridSquare, value => WorkedGrid = value);
        ClearAutoField(WorkedCountry, FirstNonBlank(record.DxccCountryName, record.Country), value => WorkedCountry = value);
        ClearAutoField(WorkedState, record.State, value => WorkedState = value);
        ClearAutoField(WorkedCounty, record.County, value => WorkedCounty = value);
        ClearAutoField(WorkedContinent, record.DxccContinent, value => WorkedContinent = value);
        ClearAutoField(WorkedIota, record.Iota, value => WorkedIota = value);

        if (record.DxccEntityId != 0)
        {
            ClearAutoField(WorkedDxcc, record.DxccEntityId.ToString(CultureInfo.InvariantCulture), value => WorkedDxcc = value);
        }

        if (record.HasCqZone)
        {
            ClearAutoField(WorkedCqZone, record.CqZone.ToString(CultureInfo.InvariantCulture), value => WorkedCqZone = value);
        }

        if (record.HasItuZone)
        {
            ClearAutoField(WorkedItuZone, record.ItuZone.ToString(CultureInfo.InvariantCulture), value => WorkedItuZone = value);
        }

        _lastLookupRecord = null;
    }

    private static void ClearAutoField(string current, string? appliedValue, Action<string> clear)
    {
        if (!string.IsNullOrWhiteSpace(appliedValue)
            && string.Equals(current, appliedValue, StringComparison.Ordinal))
        {
            clear(string.Empty);
        }
    }

    public void Dispose()
    {
        DisposeLookupCts();
        GC.SuppressFinalize(this);
    }

    private void DisposeLookupCts()
    {
        if (_lookupCts is null)
        {
            return;
        }

        _lookupCts.Cancel();
        _lookupCts.Dispose();
        _lookupCts = null;
    }

    private void InitializeStationTabState()
    {
        ShowAdvancedStationFields = HasAdvancedStationSnapshotValues();
    }

    private bool HasAdvancedStationSnapshotValues() =>
        !string.IsNullOrWhiteSpace(SnapshotProfileName)
        || !string.IsNullOrWhiteSpace(SnapshotArrlSection)
        || !string.IsNullOrWhiteSpace(SnapshotDxcc)
        || !string.IsNullOrWhiteSpace(SnapshotCqZone)
        || !string.IsNullOrWhiteSpace(SnapshotItuZone)
        || !string.IsNullOrWhiteSpace(SnapshotLatitude)
        || !string.IsNullOrWhiteSpace(SnapshotLongitude);

    private bool TryBuildQso(out QsoRecord qso, out string? error)
    {
        error = null;
        var working = _sourceQso?.Clone() ?? new QsoRecord();

        if (!TryParseTimestamp(UtcStartText, required: true, out var utcStart))
        {
            qso = new QsoRecord();
            error = "Invalid UTC start. Use yyyy-MM-dd HH:mm or an ISO-8601 timestamp.";
            return false;
        }

        var workedCallsign = NormalizeToken(WorkedCallsign, uppercase: true);
        if (workedCallsign.Length == 0)
        {
            qso = new QsoRecord();
            error = "Callsign is required.";
            return false;
        }

        if (!ProtoEnumDisplay.TryParseBand(SelectedBand, out var band))
        {
            qso = new QsoRecord();
            error = $"Invalid band: {SelectedBand}.";
            return false;
        }

        if (!ProtoEnumDisplay.TryParseMode(SelectedMode, out var mode))
        {
            qso = new QsoRecord();
            error = $"Invalid mode: {SelectedMode}.";
            return false;
        }

        working.WorkedCallsign = workedCallsign;
        working.StationCallsign = NormalizeToken(StationCallsign, uppercase: true);
        working.UtcTimestamp = Timestamp.FromDateTimeOffset(utcStart);
        working.Band = band;
        working.Mode = mode;

        if (!TryApplyOptionalTimestamp(
                UtcEndText,
                required: false,
                value => working.UtcEndTimestamp = Timestamp.FromDateTimeOffset(value),
                () => working.UtcEndTimestamp = null,
                "UTC end",
                out error)
            || !TryApplyFrequency(
                FrequencyMhz,
                hz =>
                {
                    working.FrequencyHz = hz;
#pragma warning disable CS0612
                    working.FrequencyKhz = (hz + 500) / 1000;
#pragma warning restore CS0612
                },
                () =>
                {
                    working.ClearFrequencyHz();
#pragma warning disable CS0612
                    working.ClearFrequencyKhz();
#pragma warning restore CS0612
                },
                out error)
            || !TryApplyOptionalRst(
                RstSent,
                value => working.RstSent = value,
                () => working.RstSent = null,
                "RST sent",
                out error)
            || !TryApplyOptionalRst(
                RstReceived,
                value => working.RstReceived = value,
                () => working.RstReceived = null,
                "RST received",
                out error)
            || !TryApplyOptionalTimestamp(
                QslSentDateText,
                required: false,
                value => working.QslSentDate = Timestamp.FromDateTimeOffset(value),
                () => working.QslSentDate = null,
                "QSL sent date",
                out error)
            || !TryApplyOptionalTimestamp(
                QslReceivedDateText,
                required: false,
                value => working.QslReceivedDate = Timestamp.FromDateTimeOffset(value),
                () => working.QslReceivedDate = null,
                "QSL received date",
                out error)
            || !TryApplyOptionalUInt(WorkedDxcc, "DXCC", value => working.WorkedDxcc = value, working.ClearWorkedDxcc, out error)
            || !TryApplyOptionalUInt(WorkedCqZone, "CQ zone", value => working.WorkedCqZone = value, working.ClearWorkedCqZone, out error)
            || !TryApplyOptionalUInt(WorkedItuZone, "ITU zone", value => working.WorkedItuZone = value, working.ClearWorkedItuZone, out error))
        {
            qso = new QsoRecord();
            return false;
        }

        ApplyOptionalString(Submode, value => working.Submode = value, working.ClearSubmode, uppercase: true);
        ApplyOptionalString(TxPower, value => working.TxPower = value, working.ClearTxPower);
        ApplyOptionalString(WorkedOperatorCallsign, value => working.WorkedOperatorCallsign = value, working.ClearWorkedOperatorCallsign, uppercase: true);
        ApplyOptionalString(WorkedOperatorName, value => working.WorkedOperatorName = value, working.ClearWorkedOperatorName);
        ApplyOptionalString(WorkedGrid, value => working.WorkedGrid = value, working.ClearWorkedGrid, uppercase: true);
        ApplyOptionalString(WorkedCountry, value => working.WorkedCountry = value, working.ClearWorkedCountry);
        ApplyOptionalString(WorkedState, value => working.WorkedState = value, working.ClearWorkedState);
        ApplyOptionalString(WorkedCounty, value => working.WorkedCounty = value, working.ClearWorkedCounty);
        ApplyOptionalString(WorkedIota, value => working.WorkedIota = value, working.ClearWorkedIota, uppercase: true);
        ApplyOptionalString(WorkedContinent, value => working.WorkedContinent = value, working.ClearWorkedContinent, uppercase: true);
        ApplyOptionalString(WorkedArrlSection, value => working.WorkedArrlSection = value, working.ClearWorkedArrlSection, uppercase: true);
        ApplyOptionalString(Skcc, value => working.Skcc = value, working.ClearSkcc, uppercase: true);
        ApplyOptionalString(QrzLogId, value => working.QrzLogid = value, working.ClearQrzLogid);
        ApplyOptionalString(QrzBookId, value => working.QrzBookid = value, working.ClearQrzBookid);
        ApplyOptionalString(ContestId, value => working.ContestId = value, working.ClearContestId);
        ApplyOptionalString(SerialSent, value => working.SerialSent = value, working.ClearSerialSent);
        ApplyOptionalString(SerialReceived, value => working.SerialReceived = value, working.ClearSerialReceived);
        ApplyOptionalString(ExchangeSent, value => working.ExchangeSent = value, working.ClearExchangeSent);
        ApplyOptionalString(ExchangeReceived, value => working.ExchangeReceived = value, working.ClearExchangeReceived);
        ApplyOptionalString(PropMode, value => working.PropMode = value, working.ClearPropMode, uppercase: true);
        ApplyOptionalString(SatName, value => working.SatName = value, working.ClearSatName, uppercase: true);
        ApplyOptionalString(SatMode, value => working.SatMode = value, working.ClearSatMode, uppercase: true);
        ApplyOptionalString(Notes, value => working.Notes = value, working.ClearNotes);
        ApplyOptionalString(Comment, value => working.Comment = value, working.ClearComment);
        ApplyOptionalString(CwDecodeTranscript, value => working.CwDecodeTranscript = value, working.ClearCwDecodeTranscript);
        if (uint.TryParse(CwDecodeRxWpmText, NumberStyles.None, CultureInfo.InvariantCulture, out var parsedRxWpm) && parsedRxWpm > 0)
        {
            working.CwDecodeRxWpm = parsedRxWpm;
        }
        else
        {
            working.ClearCwDecodeRxWpm();
        }

        working.QslSentStatus = ParseQslStatus(SelectedQslSentStatus);
        working.QslReceivedStatus = ParseQslStatus(SelectedQslReceivedStatus);
        ApplyOptionalBool(LotwSent, value => working.LotwSent = value, working.ClearLotwSent);
        ApplyOptionalBool(LotwReceived, value => working.LotwReceived = value, working.ClearLotwReceived);
        ApplyOptionalBool(EqslSent, value => working.EqslSent = value, working.ClearEqslSent);
        ApplyOptionalBool(EqslReceived, value => working.EqslReceived = value, working.ClearEqslReceived);

        if (!TryApplyStationSnapshot(working, out error) || !TryApplyExtraFields(working, out error))
        {
            qso = new QsoRecord();
            return false;
        }

        qso = working;
        return true;
    }

    private bool TryApplyStationSnapshot(QsoRecord qso, out string? error)
    {
        error = null;
        var hasAnySnapshotValue = !string.IsNullOrWhiteSpace(SnapshotProfileName)
            || !string.IsNullOrWhiteSpace(SnapshotStationCallsign)
            || !string.IsNullOrWhiteSpace(SnapshotOperatorCallsign)
            || !string.IsNullOrWhiteSpace(SnapshotOperatorName)
            || !string.IsNullOrWhiteSpace(SnapshotGrid)
            || !string.IsNullOrWhiteSpace(SnapshotCounty)
            || !string.IsNullOrWhiteSpace(SnapshotState)
            || !string.IsNullOrWhiteSpace(SnapshotCountry)
            || !string.IsNullOrWhiteSpace(SnapshotDxcc)
            || !string.IsNullOrWhiteSpace(SnapshotCqZone)
            || !string.IsNullOrWhiteSpace(SnapshotItuZone)
            || !string.IsNullOrWhiteSpace(SnapshotLatitude)
            || !string.IsNullOrWhiteSpace(SnapshotLongitude)
            || !string.IsNullOrWhiteSpace(SnapshotArrlSection);

        if (!hasAnySnapshotValue)
        {
            qso.StationSnapshot = null;
            return true;
        }

        var snapshot = qso.StationSnapshot?.Clone() ?? new StationSnapshot();
        snapshot.StationCallsign = NormalizeToken(SnapshotStationCallsign, uppercase: true);

        ApplyOptionalString(SnapshotProfileName, value => snapshot.ProfileName = value, snapshot.ClearProfileName);
        ApplyOptionalString(
            SnapshotOperatorCallsign,
            value => snapshot.OperatorCallsign = value,
            snapshot.ClearOperatorCallsign,
            uppercase: true);
        ApplyOptionalString(SnapshotOperatorName, value => snapshot.OperatorName = value, snapshot.ClearOperatorName);
        ApplyOptionalString(SnapshotGrid, value => snapshot.Grid = value, snapshot.ClearGrid, uppercase: true);
        ApplyOptionalString(SnapshotCounty, value => snapshot.County = value, snapshot.ClearCounty);
        ApplyOptionalString(SnapshotState, value => snapshot.State = value, snapshot.ClearState);
        ApplyOptionalString(SnapshotCountry, value => snapshot.Country = value, snapshot.ClearCountry);
        ApplyOptionalString(
            SnapshotArrlSection,
            value => snapshot.ArrlSection = value,
            snapshot.ClearArrlSection,
            uppercase: true);

        if (!TryApplyOptionalUInt(SnapshotDxcc, "snapshot DXCC", value => snapshot.Dxcc = value, snapshot.ClearDxcc, out error)
            || !TryApplyOptionalUInt(SnapshotCqZone, "snapshot CQ zone", value => snapshot.CqZone = value, snapshot.ClearCqZone, out error)
            || !TryApplyOptionalUInt(SnapshotItuZone, "snapshot ITU zone", value => snapshot.ItuZone = value, snapshot.ClearItuZone, out error)
            || !TryApplyOptionalDouble(
                SnapshotLatitude,
                "snapshot latitude",
                value => snapshot.Latitude = value,
                snapshot.ClearLatitude,
                out error)
            || !TryApplyOptionalDouble(
                SnapshotLongitude,
                "snapshot longitude",
                value => snapshot.Longitude = value,
                snapshot.ClearLongitude,
                out error))
        {
            return false;
        }

        qso.StationSnapshot = snapshot;
        return true;
    }

    private bool TryApplyExtraFields(QsoRecord qso, out string? error)
    {
        error = null;
        qso.ExtraFields.Clear();

        if (string.IsNullOrWhiteSpace(ExtraFieldsText))
        {
            return true;
        }

        var lines = ExtraFieldsText.Replace("\r", string.Empty, StringComparison.Ordinal).Split('\n');
        foreach (var rawLine in lines)
        {
            var line = rawLine.Trim();
            if (line.Length == 0)
            {
                continue;
            }

            var separatorIndex = line.IndexOf('=', StringComparison.Ordinal);
            if (separatorIndex <= 0)
            {
                error = $"Invalid extra field entry: {line}. Use KEY=value.";
                return false;
            }

            var key = NormalizeToken(line[..separatorIndex], uppercase: true);
            if (key.Length == 0)
            {
                error = $"Invalid extra field key: {line}.";
                return false;
            }

            qso.ExtraFields[key] = line[(separatorIndex + 1)..].Trim();
        }

        return true;
    }

    private static bool TryApplyFrequency(
        string value,
        Action<ulong> setter,
        Action clearer,
        out string? error)
    {
        error = null;
        var normalized = NoteOrNull(value);
        if (normalized is null)
        {
            clearer();
            return true;
        }

        if (double.TryParse(normalized, NumberStyles.Float, CultureInfo.InvariantCulture, out var mhz) && mhz > 0)
        {
            setter((ulong)Math.Round(mhz * 1_000_000.0, MidpointRounding.AwayFromZero));
            return true;
        }

        error = $"Invalid frequency: {value}. Use MHz such as 14.074.";
        return false;
    }

    private static string FormatFrequencyMhz(ulong hz)
    {
        ulong whole = hz / 1_000_000;
        ulong frac = hz % 1_000_000;
        string full = $"{whole}.{frac:000000}";
        int dotPos = full.IndexOf('.', StringComparison.Ordinal);
        int minLen = dotPos + 4; // dot + 3 digits minimum
        var trimmed = full.AsSpan().TrimEnd('0');
        int end = Math.Max(trimmed.Length, minLen);
        return full[..end];
    }

    private static bool TryApplyOptionalRst(
        string value,
        Action<RstReport> setter,
        Action clearer,
        string fieldName,
        out string? error)
    {
        error = null;
        var normalized = NoteOrNull(value);
        if (normalized is null)
        {
            clearer();
            return true;
        }

        if (!TryParseRstToken(normalized, out var report))
        {
            error = $"Invalid {fieldName}: {value}. Use 59 or 599.";
            return false;
        }

        setter(report);
        return true;
    }

    private static bool TryApplyOptionalTimestamp(
        string value,
        bool required,
        Action<DateTimeOffset> setter,
        Action clearer,
        string fieldName,
        out string? error)
    {
        error = null;
        if (TryParseTimestamp(value, required, out var parsed))
        {
            if (parsed == DateTimeOffset.MinValue)
            {
                clearer();
            }
            else
            {
                setter(parsed);
            }

            return true;
        }

        error = $"Invalid {fieldName}: {value}.";
        return false;
    }

    private static bool TryApplyOptionalUInt(
        string value,
        string fieldName,
        Action<uint> setter,
        Action clearer,
        out string? error)
    {
        error = null;
        var normalized = NoteOrNull(value);
        if (normalized is null)
        {
            clearer();
            return true;
        }

        if (uint.TryParse(normalized, NumberStyles.None, CultureInfo.InvariantCulture, out var parsed))
        {
            setter(parsed);
            return true;
        }

        error = $"Invalid {fieldName}: {value}.";
        return false;
    }

    private static bool TryApplyOptionalDouble(
        string value,
        string fieldName,
        Action<double> setter,
        Action clearer,
        out string? error)
    {
        error = null;
        var normalized = NoteOrNull(value);
        if (normalized is null)
        {
            clearer();
            return true;
        }

        if (double.TryParse(normalized, NumberStyles.Float, CultureInfo.InvariantCulture, out var parsed))
        {
            setter(parsed);
            return true;
        }

        error = $"Invalid {fieldName}: {value}.";
        return false;
    }

    private static void ApplyOptionalBool(bool? value, Action<bool> setter, Action clearer)
    {
        if (value.HasValue)
        {
            setter(value.Value);
        }
        else
        {
            clearer();
        }
    }

    private static bool TryParseTimestamp(string? value, bool required, out DateTimeOffset timestamp)
    {
        timestamp = DateTimeOffset.MinValue;
        var normalized = NoteOrNull(value);
        if (normalized is null)
        {
            return !required;
        }

        return DateTimeOffset.TryParseExact(
                   normalized,
                   TimestampFormats,
                   CultureInfo.InvariantCulture,
                   DateTimeStyles.AssumeUniversal | DateTimeStyles.AdjustToUniversal,
                   out timestamp)
               || DateTimeOffset.TryParse(
                   normalized,
                   CultureInfo.InvariantCulture,
                   DateTimeStyles.AssumeUniversal | DateTimeStyles.AdjustToUniversal,
                   out timestamp);
    }

    private static bool TryParseRstToken(string value, out RstReport report)
    {
        report = new RstReport();
        var normalized = NormalizeToken(value, uppercase: false);
        if (normalized.Length is not (2 or 3) || normalized.Any(static c => !char.IsAsciiDigit(c)))
        {
            return false;
        }

        report.Readability = (uint)(normalized[0] - '0');
        report.Strength = (uint)(normalized[1] - '0');
        if (normalized.Length == 3)
        {
            report.Tone = (uint)(normalized[2] - '0');
        }

        return true;
    }

    private static string FormatTimestamp(DateTimeOffset value) =>
        value.ToUniversalTime().ToString("yyyy-MM-dd HH:mm", CultureInfo.InvariantCulture);

    private static string FormatTimestamp(Timestamp? value) =>
        value is null ? string.Empty : FormatTimestamp(value.ToDateTimeOffset());

    private static string FormatDate(Timestamp? value) =>
        value is null
            ? string.Empty
            : value.ToDateTimeOffset().ToUniversalTime().ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);

    private static string FormatRst(RstReport? report)
    {
        if (report is null)
        {
            return string.Empty;
        }

        return report.Tone == 0
            ? string.Create(
                CultureInfo.InvariantCulture,
                $"{report.Readability}{report.Strength}")
            : string.Create(
                CultureInfo.InvariantCulture,
                $"{report.Readability}{report.Strength}{report.Tone}");
    }

    private static string FormatQslStatus(QslStatus value) =>
        value switch
        {
            QslStatus.No => "No",
            QslStatus.Yes => "Yes",
            QslStatus.Requested => "Requested",
            QslStatus.Queued => "Queued",
            QslStatus.Ignore => "Ignore",
            _ => "-"
        };

    private static QslStatus ParseQslStatus(string? value) =>
        NormalizeToken(value, uppercase: true) switch
        {
            "NO" => QslStatus.No,
            "YES" => QslStatus.Yes,
            "REQUESTED" => QslStatus.Requested,
            "QUEUED" => QslStatus.Queued,
            "IGNORE" => QslStatus.Ignore,
            _ => QslStatus.Unspecified
        };

    private static void ApplyOptionalString(
        string value,
        Action<string> setter,
        Action clearer,
        bool uppercase = false)
    {
        var normalized = NormalizeToken(value, uppercase);
        if (normalized.Length == 0)
        {
            clearer();
            return;
        }

        setter(normalized);
    }

    private static string NormalizeToken(string? value, bool uppercase = false)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return string.Empty;
        }

        var normalized = value.Trim();
        return uppercase ? normalized.ToUpperInvariant() : normalized;
    }

    private static string? NoteOrNull(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();

    private static string BuildSyncStatus(SyncStatus value) =>
        value switch
        {
            SyncStatus.LocalOnly => "Local",
            SyncStatus.Synced => "Synced",
            SyncStatus.Modified => "Modified",
            SyncStatus.Conflict => "Conflict",
            _ => "Local"
        };

    private static string? FirstNonBlank(params string?[] values)
    {
        foreach (var value in values)
        {
            var normalized = NoteOrNull(value);
            if (normalized is not null)
            {
                return normalized;
            }
        }

        return null;
    }

    private static string BuildName(CallsignRecord record) =>
        FirstNonBlank(record.FormattedName, string.Join(" ", new[] { record.FirstName, record.LastName }.Where(static part => !string.IsNullOrWhiteSpace(part))))
        ?? string.Empty;

    private static string FormatExtraFields(MapField<string, string> extraFields)
    {
        if (extraFields.Count == 0)
        {
            return string.Empty;
        }

        return string.Join(
            Environment.NewLine,
            extraFields
                .OrderBy(static pair => pair.Key, StringComparer.Ordinal)
                .Select(static pair => $"{pair.Key}={pair.Value}"));
    }
}
