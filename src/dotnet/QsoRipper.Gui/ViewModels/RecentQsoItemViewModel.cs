using System.Globalization;
using System.Linq;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.ViewModels;

internal sealed class RecentQsoItemViewModel
{
    public string LocalId { get; init; } = string.Empty;

    public string UtcDisplay { get; init; } = "-";

    public string WorkedCallsign { get; init; } = "-";

    public string Band { get; init; } = "-";

    public string Mode { get; init; } = "-";

    public string Country { get; init; } = "-";

    public string OperatorName { get; init; } = "-";

    public string Frequency { get; init; } = "-";

    public string RstSent { get; init; } = "-";

    public string RstReceived { get; init; } = "-";

    public string Grid { get; init; } = "-";

    public string Comment { get; init; } = "-";

    public string UtcEndDisplay { get; init; } = "-";

    public string SyncStatus { get; init; } = "-";

    internal string SearchDocument { get; init; } = string.Empty;

    internal DateTimeOffset UtcSortKey { get; init; }

    internal string CallsignSortKey { get; init; } = string.Empty;

    internal string BandSortKey { get; init; } = string.Empty;

    internal string ModeSortKey { get; init; } = string.Empty;

    internal string CountrySortKey { get; init; } = string.Empty;

    internal string NameSortKey { get; init; } = string.Empty;

    internal ulong FrequencySortKey { get; init; }

    internal string RstSentSortKey { get; init; } = string.Empty;

    internal string RstReceivedSortKey { get; init; } = string.Empty;

    internal string GridSortKey { get; init; } = string.Empty;

    internal string CommentSortKey { get; init; } = string.Empty;

    internal DateTimeOffset UtcEndSortKey { get; init; }

    internal string SyncStatusSortKey { get; init; } = string.Empty;

    public static RecentQsoItemViewModel FromQso(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        var utcSortKey = qso.UtcTimestamp?.ToDateTimeOffset().ToUniversalTime() ?? DateTimeOffset.MinValue;
        var utcEndSortKey = qso.UtcEndTimestamp?.ToDateTimeOffset().ToUniversalTime() ?? DateTimeOffset.MinValue;
        var band = ProtoEnumDisplay.ForBand(qso.Band);
        var mode = ProtoEnumDisplay.ForMode(qso.Mode);
        var country = BuildCountry(qso);
        var operatorName = BuildOperatorName(qso);
        var grid = DisplayOrDash(qso.WorkedGrid);
        var frequency = qso.HasFrequencyKhz
            ? qso.FrequencyKhz.ToString("N0", CultureInfo.InvariantCulture)
            : "-";
        var rstSent = DisplayOrDash(qso.RstSent?.Raw);
        var rstReceived = DisplayOrDash(qso.RstReceived?.Raw);
        var comment = BuildComment(qso);
        var syncStatus = BuildSyncStatus(qso.SyncStatus);

        return new RecentQsoItemViewModel
        {
            LocalId = qso.LocalId,
            UtcDisplay = FormatTimestamp(qso.UtcTimestamp),
            WorkedCallsign = DisplayOrDash(qso.WorkedCallsign),
            Band = band,
            Mode = mode,
            Country = country,
            OperatorName = operatorName,
            Frequency = frequency,
            RstSent = rstSent,
            RstReceived = rstReceived,
            Grid = grid,
            Comment = comment,
            UtcEndDisplay = FormatTimestamp(qso.UtcEndTimestamp),
            SyncStatus = syncStatus,
            SearchDocument = BuildSearchDocument(
                qso.WorkedCallsign,
                operatorName,
                band,
                mode,
                qso.Submode,
                frequency,
                qso.WorkedCountry,
                qso.WorkedState,
                qso.WorkedCounty,
                qso.WorkedContinent,
                qso.WorkedDxcc.ToString(CultureInfo.InvariantCulture),
                qso.WorkedCqZone.ToString(CultureInfo.InvariantCulture),
                qso.WorkedItuZone.ToString(CultureInfo.InvariantCulture),
                grid,
                rstSent,
                rstReceived,
                qso.TxPower,
                qso.ContestId,
                qso.SerialSent,
                qso.SerialReceived,
                qso.ExchangeSent,
                qso.ExchangeReceived,
                qso.PropMode,
                qso.SatName,
                qso.SatMode,
                qso.WorkedIota,
                qso.WorkedArrlSection,
                comment,
                syncStatus,
                FormatTimestamp(qso.UtcTimestamp),
                FormatTimestamp(qso.UtcEndTimestamp)),
            UtcSortKey = utcSortKey,
            CallsignSortKey = NormalizeSortValue(qso.WorkedCallsign),
            BandSortKey = NormalizeSortValue(band),
            ModeSortKey = NormalizeSortValue(mode),
            CountrySortKey = NormalizeSortValue(country),
            NameSortKey = NormalizeSortValue(operatorName),
            FrequencySortKey = qso.HasFrequencyKhz ? qso.FrequencyKhz : 0,
            RstSentSortKey = NormalizeSortValue(rstSent),
            RstReceivedSortKey = NormalizeSortValue(rstReceived),
            GridSortKey = NormalizeSortValue(grid),
            CommentSortKey = NormalizeSortValue(comment),
            UtcEndSortKey = utcEndSortKey,
            SyncStatusSortKey = NormalizeSortValue(syncStatus)
        };
    }

    private static string BuildComment(QsoRecord qso)
    {
        var parts = new[]
            {
                TrimOrNull(qso.Comment),
                TrimOrNull(qso.Notes),
                TrimOrNull(qso.ContestId)
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

    private static string DisplayOrDash(string? value) =>
        string.IsNullOrWhiteSpace(value) ? "-" : value.Trim();

    private static string? FirstNonBlank(params string?[] values)
    {
        foreach (var value in values)
        {
            var trimmed = TrimOrNull(value);
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

    private static string NormalizeSortValue(string? value) =>
        string.IsNullOrWhiteSpace(value) || value == "-"
            ? string.Empty
            : value.Trim().ToUpperInvariant();

    private static string? TrimOrNull(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
