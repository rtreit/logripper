using System.ComponentModel;
using System.Globalization;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.ViewModels;

internal sealed class RecentQsoItemViewModel : ObservableObject, IEditableObject
{
    private static readonly string[] TimestampFormats =
    [
        "yy-MM-dd HH:mm",
        "yyyy-MM-dd HH:mm",
        "yyyy-MM-dd HH:mm:ss",
        "yyyy-MM-ddTHH:mm",
        "yyyy-MM-ddTHH:mm:ss",
        "yyyy-MM-ddTHH:mm:ssZ",
        "yyyy-MM-ddTHH:mm:ss.fffZ",
        "o"
    ];

    private QsoRecord _sourceQso = new();
    private EditableQsoState? _editSnapshot;
    private string _utcDisplay = "-";
    private string _workedCallsign = "-";
    private string _band = "-";
    private string _mode = "-";
    private string _country = "-";
    private string _operatorName = "-";
    private string _frequency = "-";
    private string _rst = "-";
    private string _dxcc = "-";
    private string _grid = "-";
    private string _exchange = "-";
    private string _contest = "-";
    private string _station = "-";
    private string _note = "-";
    private string _utcEndDisplay = "-";
    private string _cqZone = "-";
    private string _ituZone = "-";
    private string _qth = "-";
    private string _syncStatus = "-";
    private bool _isDirty;

    public string LocalId => _sourceQso.LocalId;

    public string UtcDisplay
    {
        get => _utcDisplay;
        set => SetProperty(ref _utcDisplay, value);
    }

    public string WorkedCallsign
    {
        get => _workedCallsign;
        set => SetProperty(ref _workedCallsign, value);
    }

    public string Band
    {
        get => _band;
        set => SetProperty(ref _band, value);
    }

    public string Mode
    {
        get => _mode;
        set => SetProperty(ref _mode, value);
    }

    public string Country
    {
        get => _country;
        set => SetProperty(ref _country, value);
    }

    public string OperatorName
    {
        get => _operatorName;
        set => SetProperty(ref _operatorName, value);
    }

    public string Frequency
    {
        get => _frequency;
        set => SetProperty(ref _frequency, value);
    }

    public string Rst
    {
        get => _rst;
        set => SetProperty(ref _rst, value);
    }

    public string RstSent => SplitCombinedReport(Rst).Sent;

    public string RstReceived => SplitCombinedReport(Rst).Received;

    public string Dxcc
    {
        get => _dxcc;
        set => SetProperty(ref _dxcc, value);
    }

    public string Grid
    {
        get => _grid;
        set => SetProperty(ref _grid, value);
    }

    public string Exchange
    {
        get => _exchange;
        set => SetProperty(ref _exchange, value);
    }

    public string Contest
    {
        get => _contest;
        set => SetProperty(ref _contest, value);
    }

    public string Station
    {
        get => _station;
        set => SetProperty(ref _station, value);
    }

    public string Note
    {
        get => _note;
        set => SetProperty(ref _note, value);
    }

    public string UtcEndDisplay
    {
        get => _utcEndDisplay;
        set => SetProperty(ref _utcEndDisplay, value);
    }

    public string CqZone
    {
        get => _cqZone;
        set => SetProperty(ref _cqZone, value);
    }

    public string ItuZone
    {
        get => _ituZone;
        set => SetProperty(ref _ituZone, value);
    }

    public string Qth
    {
        get => _qth;
        private set => SetProperty(ref _qth, value);
    }

    public string SyncStatus
    {
        get => _syncStatus;
        private set => SetProperty(ref _syncStatus, value);
    }

    public bool IsDirty
    {
        get => _isDirty;
        private set => SetProperty(ref _isDirty, value);
    }

    internal string SearchDocument => BuildSearchDocument(
        WorkedCallsign,
        OperatorName,
        Band,
        Mode,
        _sourceQso.Submode,
        Frequency,
        Country,
        Grid,
        Qth,
        Rst,
        RstSent,
        RstReceived,
        Dxcc,
        Contest,
        Exchange,
        Station,
        Note,
        CqZone,
        ItuZone,
        SyncStatus,
        _sourceQso.WorkedState,
        _sourceQso.WorkedCounty,
        _sourceQso.WorkedContinent,
        _sourceQso.TxPower,
        _sourceQso.SerialSent,
        _sourceQso.SerialReceived,
        _sourceQso.ExchangeSent,
        _sourceQso.ExchangeReceived,
        _sourceQso.PropMode,
        _sourceQso.SatName,
        _sourceQso.SatMode,
        _sourceQso.WorkedIota,
        _sourceQso.WorkedArrlSection,
        UtcDisplay,
        UtcEndDisplay);

    public DateTimeOffset UtcSortKey =>
        TryParseTimestamp(UtcDisplay, required: false, out var timestamp)
            ? timestamp
            : DateTimeOffset.MinValue;

    public ulong FrequencySortKey =>
        TryParseFrequency(Frequency, out var frequency)
            ? frequency
            : 0;

    public uint DxccSortKey =>
        TryParseOptionalUInt(Dxcc, out var dxcc)
            ? dxcc
            : 0;

    public DateTimeOffset UtcEndSortKey =>
        TryParseTimestamp(UtcEndDisplay, required: false, out var timestamp)
            ? timestamp
            : DateTimeOffset.MinValue;

    public static RecentQsoItemViewModel FromQso(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        var item = new RecentQsoItemViewModel();
        item.LoadSourceQso(qso);
        return item;
    }

    public void BeginEdit()
    {
        _editSnapshot ??= CaptureState();
    }

    public void CancelEdit()
    {
        if (_editSnapshot is not { } snapshot)
        {
            return;
        }

        _editSnapshot = null;
        ApplyState(snapshot);
        RecomputeDirty();
    }

    public void EndEdit()
    {
        _editSnapshot = null;
        RecomputeDirty();
    }

    internal void AcceptSavedChanges(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        _sourceQso = qso.Clone();
        _editSnapshot = null;
        Qth = BuildQth(_sourceQso);
        SyncStatus = BuildSyncStatus(_sourceQso.SyncStatus);
        RecomputeDirty();
    }

    internal bool MatchesFieldToken(string key, string value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(key);
        ArgumentException.ThrowIfNullOrWhiteSpace(value);

        var normalizedValue = NormalizeSearchValue(value);
        return key switch
        {
            "CALL" or "CALLSIGN" => ContainsNormalized(WorkedCallsign, normalizedValue),
            "BAND" => ContainsNormalized(Band, normalizedValue),
            "MODE" => ContainsNormalized(Mode, normalizedValue),
            "FREQ" or "FREQUENCY" => ContainsNormalized(Frequency, normalizedValue),
            "RST" => ContainsNormalized(Rst, normalizedValue)
                || ContainsNormalized(RstSent, normalizedValue)
                || ContainsNormalized(RstReceived, normalizedValue),
            "DXCC" => ContainsNormalized(Dxcc, normalizedValue),
            "COUNTRY" => ContainsNormalized(Country, normalizedValue),
            "NAME" => ContainsNormalized(OperatorName, normalizedValue),
            "GRID" => ContainsNormalized(Grid, normalizedValue),
            "EXCH" or "EXCHANGE" => ContainsNormalized(Exchange, normalizedValue),
            "CONTEST" => ContainsNormalized(Contest, normalizedValue),
            "STATION" => ContainsNormalized(Station, normalizedValue),
            "NOTE" or "COMMENT" => ContainsNormalized(Note, normalizedValue),
            "CQ" => ContainsNormalized(CqZone, normalizedValue),
            "ITU" => ContainsNormalized(ItuZone, normalizedValue),
            "QTH" => ContainsNormalized(Qth, normalizedValue),
            "SYNC" => ContainsNormalized(SyncStatus, normalizedValue),
            _ => false
        };
    }

    internal bool TryBuildUpdatedQso(out QsoRecord? qso, out string? error)
    {
        error = null;
        qso = null;

        var updated = _sourceQso.Clone();
        var sourceState = EditableQsoState.FromQso(_sourceQso);

        if (!TryParseTimestamp(UtcDisplay, required: true, out var utcTimestamp))
        {
            error = "Invalid UTC value. Use yy-MM-dd HH:mm or an ISO-8601 timestamp.";
            return false;
        }

        updated.UtcTimestamp = Timestamp.FromDateTimeOffset(utcTimestamp);

        var workedCallsign = NormalizeToken(WorkedCallsign, uppercase: true);
        if (workedCallsign.Length == 0)
        {
            error = "Call is required.";
            return false;
        }

        updated.WorkedCallsign = workedCallsign;

        if (!ProtoEnumDisplay.TryParseBand(Band, out var band))
        {
            error = $"Invalid band: {Band}.";
            return false;
        }

        if (!ProtoEnumDisplay.TryParseMode(Mode, out var mode))
        {
            error = $"Invalid mode: {Mode}.";
            return false;
        }

        updated.Band = band;
        updated.Mode = mode;

        if (!TryApplyFrequency(updated, out error)
            || (!StringComparer.Ordinal.Equals(Rst, sourceState.Rst) && !TryApplyRst(updated, out error))
            || !TryApplyOptionalUInt(Dxcc, "DXCC", value => updated.WorkedDxcc = value, updated.ClearWorkedDxcc, out error)
            || !TryApplyOptionalUInt(CqZone, "CQ zone", value => updated.WorkedCqZone = value, updated.ClearWorkedCqZone, out error)
            || !TryApplyOptionalUInt(ItuZone, "ITU zone", value => updated.WorkedItuZone = value, updated.ClearWorkedItuZone, out error)
            || !TryApplyUtcEnd(updated, out error))
        {
            return false;
        }

        if (!StringComparer.Ordinal.Equals(Country, sourceState.Country))
        {
            ApplyOptionalString(Country, value => updated.WorkedCountry = value, updated.ClearWorkedCountry);
        }

        if (!StringComparer.Ordinal.Equals(OperatorName, sourceState.OperatorName))
        {
            ApplyOptionalString(
                OperatorName,
                value => updated.WorkedOperatorName = value,
                () =>
                {
                    updated.ClearWorkedOperatorName();
                    updated.ClearWorkedOperatorCallsign();
                });
        }

        ApplyOptionalString(Grid, value => updated.WorkedGrid = value, updated.ClearWorkedGrid, uppercase: true);
        ApplyOptionalString(Contest, value => updated.ContestId = value, updated.ClearContestId);
        ApplyOptionalString(Station, value => updated.StationCallsign = value, () => updated.StationCallsign = string.Empty, uppercase: true);

        if (!StringComparer.Ordinal.Equals(Exchange, sourceState.Exchange))
        {
            ApplyExchange(updated);
        }

        if (!StringComparer.Ordinal.Equals(Note, sourceState.Note))
        {
            ApplyNote(updated);
        }

        qso = updated;
        return true;
    }

    private void LoadSourceQso(QsoRecord qso)
    {
        _sourceQso = qso.Clone();
        _editSnapshot = null;
        ApplyState(EditableQsoState.FromQso(_sourceQso));
        Qth = BuildQth(_sourceQso);
        SyncStatus = BuildSyncStatus(_sourceQso.SyncStatus);
        RecomputeDirty();
    }

    private EditableQsoState CaptureState() => new(
        UtcDisplay,
        WorkedCallsign,
        Band,
        Mode,
        Frequency,
        Rst,
        Dxcc,
        Country,
        OperatorName,
        Grid,
        Exchange,
        Contest,
        Station,
        Note,
        UtcEndDisplay,
        CqZone,
        ItuZone);

    private void ApplyState(EditableQsoState state)
    {
        UtcDisplay = state.UtcDisplay;
        WorkedCallsign = state.WorkedCallsign;
        Band = state.Band;
        Mode = state.Mode;
        Frequency = state.Frequency;
        Rst = state.Rst;
        Dxcc = state.Dxcc;
        Country = state.Country;
        OperatorName = state.OperatorName;
        Grid = state.Grid;
        Exchange = state.Exchange;
        Contest = state.Contest;
        Station = state.Station;
        Note = state.Note;
        UtcEndDisplay = state.UtcEndDisplay;
        CqZone = state.CqZone;
        ItuZone = state.ItuZone;
    }

    private void RecomputeDirty()
    {
        IsDirty = CaptureState() != EditableQsoState.FromQso(_sourceQso);
    }

    private bool TryApplyFrequency(QsoRecord updated, out string? error)
    {
        error = null;

        if (TryParseFrequency(Frequency, out var frequency))
        {
            updated.FrequencyKhz = frequency;
            return true;
        }

        if (string.IsNullOrWhiteSpace(Frequency) || Frequency == "-")
        {
            updated.ClearFrequencyKhz();
            return true;
        }

        error = $"Invalid frequency: {Frequency}.";
        return false;
    }

    private bool TryApplyRst(QsoRecord updated, out string? error)
    {
        error = null;

        var value = NoteOrNull(Rst);
        if (value is null)
        {
            updated.RstSent = null;
            updated.RstReceived = null;
            return true;
        }

        var parts = value.Split(['/', ' '], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        if (parts.Length is < 1 or > 2)
        {
            error = $"Invalid RST: {Rst}. Use 59 or 59/59.";
            return false;
        }

        if (!TryParseRstToken(parts[0], out var sent))
        {
            error = $"Invalid RST: {Rst}.";
            return false;
        }

        var received = sent.Clone();
        if (parts.Length == 2 && !TryParseRstToken(parts[1], out received))
        {
            error = $"Invalid RST: {Rst}.";
            return false;
        }

        updated.RstSent = sent;
        updated.RstReceived = received;
        return true;
    }

    private static bool TryApplyOptionalUInt(
        string value,
        string fieldName,
        Action<uint> setter,
        Action clearer,
        out string? error)
    {
        error = null;

        if (TryParseOptionalUInt(value, out var parsed))
        {
            if (parsed == 0)
            {
                clearer();
            }
            else
            {
                setter(parsed);
            }

            return true;
        }

        if (string.IsNullOrWhiteSpace(value) || value == "-")
        {
            clearer();
            return true;
        }

        error = $"Invalid {fieldName}: {value}.";
        return false;
    }

    private bool TryApplyUtcEnd(QsoRecord updated, out string? error)
    {
        error = null;

        if (TryParseTimestamp(UtcEndDisplay, required: false, out var utcEnd))
        {
            if (utcEnd == DateTimeOffset.MinValue)
            {
                updated.UtcEndTimestamp = null;
            }
            else
            {
                updated.UtcEndTimestamp = Timestamp.FromDateTimeOffset(utcEnd);
            }

            return true;
        }

        error = "Invalid end time. Use yy-MM-dd HH:mm or an ISO-8601 timestamp.";
        return false;
    }

    private void ApplyExchange(QsoRecord updated)
    {
        var exchange = NoteOrNull(Exchange);
        if (exchange is null)
        {
            updated.ClearExchangeReceived();
            updated.ClearExchangeSent();
            updated.ClearSerialReceived();
            updated.ClearSerialSent();
            return;
        }

        updated.ExchangeReceived = exchange;
        updated.ClearExchangeSent();
        updated.ClearSerialReceived();
        updated.ClearSerialSent();
    }

    private void ApplyNote(QsoRecord updated)
    {
        var note = NoteOrNull(Note);
        if (note is null)
        {
            updated.ClearComment();
            updated.ClearNotes();
            return;
        }

        updated.Comment = note;
        updated.ClearNotes();
    }

    private static EditableQsoState EditableQsoStateFromQso(QsoRecord qso) => new(
        FormatTimestamp(qso.UtcTimestamp),
        DisplayOrDash(qso.WorkedCallsign),
        ProtoEnumDisplay.ForBand(qso.Band),
        ProtoEnumDisplay.ForMode(qso.Mode),
        qso.HasFrequencyKhz ? qso.FrequencyKhz.ToString("N0", CultureInfo.InvariantCulture) : "-",
        BuildCombinedReport(DisplayOrDash(qso.RstSent?.Raw), DisplayOrDash(qso.RstReceived?.Raw)),
        BuildDxcc(qso),
        BuildCountry(qso),
        BuildOperatorName(qso),
        DisplayOrDash(qso.WorkedGrid),
        BuildExchange(qso),
        DisplayOrDash(qso.ContestId),
        DisplayOrDash(qso.StationCallsign),
        BuildNote(qso),
        FormatTimestamp(qso.UtcEndTimestamp),
        BuildOptionalNumber(qso.WorkedCqZone),
        BuildOptionalNumber(qso.WorkedItuZone));

    private static string BuildNote(QsoRecord qso)
    {
        var parts = new[]
            {
                NoteOrNull(qso.Comment),
                NoteOrNull(qso.Notes)
            }
            .Where(static value => value is not null)
            .Distinct(StringComparer.Ordinal)
            .ToArray();

        return parts.Length == 0 ? "-" : string.Join(" / ", parts!);
    }

    private static string BuildCountry(QsoRecord qso)
    {
        return FirstNonBlank(
                   qso.WorkedCountry,
                   qso.WorkedState,
                   qso.WorkedCounty,
                   qso.WorkedContinent)
               ?? "-";
    }

    private static string BuildOperatorName(QsoRecord qso) =>
        FirstNonBlank(qso.WorkedOperatorName, qso.WorkedOperatorCallsign) ?? "-";

    private static string BuildExchange(QsoRecord qso)
    {
        return FirstNonBlank(
                   qso.ExchangeReceived,
                   qso.SerialReceived,
                   qso.ExchangeSent,
                   qso.SerialSent)
               ?? "-";
    }

    private static string BuildQth(QsoRecord qso)
    {
        var parts = new[]
            {
                NoteOrNull(qso.WorkedState),
                NoteOrNull(qso.WorkedCounty),
                NoteOrNull(qso.WorkedContinent)
            }
            .Where(static value => value is not null)
            .Distinct(StringComparer.Ordinal)
            .ToArray();

        return parts.Length == 0 ? "-" : string.Join(" / ", parts!);
    }

    private static string BuildDxcc(QsoRecord qso) =>
        qso.WorkedDxcc == 0 ? "-" : qso.WorkedDxcc.ToString(CultureInfo.InvariantCulture);

    private static string BuildOptionalNumber(uint value) =>
        value == 0 ? "-" : value.ToString(CultureInfo.InvariantCulture);

    private static string BuildCombinedReport(string rstSent, string rstReceived)
    {
        if (rstSent == "-" && rstReceived == "-")
        {
            return "-";
        }

        if (rstSent == "-")
        {
            return rstReceived;
        }

        if (rstReceived == "-")
        {
            return rstSent;
        }

        return $"{rstSent}/{rstReceived}";
    }

    private static (string Sent, string Received) SplitCombinedReport(string value)
    {
        var normalized = NoteOrNull(value);
        if (normalized is null)
        {
            return ("-", "-");
        }

        var parts = normalized.Split(['/', ' '], StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);
        return parts.Length switch
        {
            0 => ("-", "-"),
            1 => (parts[0], parts[0]),
            _ => (parts[0], parts[1])
        };
    }

    private static string BuildSearchDocument(params string?[] values)
    {
        return string.Join(
                ' ',
                values
                    .Where(static value => !string.IsNullOrWhiteSpace(value))
                    .Select(static value => value!.Trim()))
            .ToUpperInvariant();
    }

    private static string BuildSyncStatus(SyncStatus status)
    {
        return status switch
        {
            QsoRipper.Domain.SyncStatus.LocalOnly => "Local",
            QsoRipper.Domain.SyncStatus.Synced => "Synced",
            QsoRipper.Domain.SyncStatus.Modified => "Modified",
            QsoRipper.Domain.SyncStatus.Conflict => "Conflict",
            _ => "-"
        };
    }

    private static bool TryParseTimestamp(string? value, bool required, out DateTimeOffset timestamp)
    {
        timestamp = DateTimeOffset.MinValue;
        var normalized = NoteOrNull(value);
        if (normalized is null || normalized == "-")
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

    private static bool TryParseFrequency(string? value, out ulong frequency)
    {
        frequency = 0;
        var normalized = NoteOrNull(value);
        if (normalized is null || normalized == "-")
        {
            return false;
        }

        return ulong.TryParse(
            normalized.Replace(",", string.Empty, StringComparison.Ordinal),
            NumberStyles.None,
            CultureInfo.InvariantCulture,
            out frequency);
    }

    private static bool TryParseOptionalUInt(string? value, out uint parsed)
    {
        parsed = 0;
        var normalized = NoteOrNull(value);
        if (normalized is null || normalized == "-")
        {
            return true;
        }

        return uint.TryParse(
            normalized.Replace(",", string.Empty, StringComparison.Ordinal),
            NumberStyles.None,
            CultureInfo.InvariantCulture,
            out parsed);
    }

    private static bool TryParseRstToken(string value, out RstReport report)
    {
        report = new RstReport();

        if (value.Length is not (2 or 3) || value.Any(static c => !char.IsAsciiDigit(c)))
        {
            return false;
        }

        report.Readability = (uint)(value[0] - '0');
        report.Strength = (uint)(value[1] - '0');

        if (value.Length == 3)
        {
            report.Tone = (uint)(value[2] - '0');
        }

        return true;
    }

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

    private static string DisplayOrDash(string? value) =>
        string.IsNullOrWhiteSpace(value) ? "-" : value.Trim();

    private static bool ContainsNormalized(string? candidate, string normalizedValue) =>
        NormalizeSearchValue(candidate).Contains(normalizedValue, StringComparison.Ordinal);

    private static string NormalizeSearchValue(string? value) =>
        string.IsNullOrWhiteSpace(value) || value == "-"
            ? string.Empty
            : value.Trim().ToUpperInvariant();

    private static string NormalizeToken(string? value, bool uppercase = false)
    {
        if (string.IsNullOrWhiteSpace(value) || value == "-")
        {
            return string.Empty;
        }

        var normalized = value.Trim();
        return uppercase ? normalized.ToUpperInvariant() : normalized;
    }

    private static string? NoteOrNull(string? value) =>
        string.IsNullOrWhiteSpace(value) || value == "-" ? null : value.Trim();

    private static string? FirstNonBlank(params string?[] values)
    {
        foreach (var value in values)
        {
            var trimmed = NoteOrNull(value);
            if (trimmed is not null)
            {
                return trimmed;
            }
        }

        return null;
    }

    private static string FormatTimestamp(Timestamp? timestamp)
    {
        return timestamp is null
            ? "-"
            : timestamp.ToDateTimeOffset().ToUniversalTime().ToString("yy-MM-dd HH:mm", CultureInfo.InvariantCulture);
    }

    private readonly record struct EditableQsoState(
        string UtcDisplay,
        string WorkedCallsign,
        string Band,
        string Mode,
        string Frequency,
        string Rst,
        string Dxcc,
        string Country,
        string OperatorName,
        string Grid,
        string Exchange,
        string Contest,
        string Station,
        string Note,
        string UtcEndDisplay,
        string CqZone,
        string ItuZone)
    {
        public static EditableQsoState FromQso(QsoRecord qso) => EditableQsoStateFromQso(qso);
    }
}
