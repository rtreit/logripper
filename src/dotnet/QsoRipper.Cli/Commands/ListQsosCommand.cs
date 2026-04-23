using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Services;
using static QsoRipper.Cli.EnumHelpers;

namespace QsoRipper.Cli.Commands;

internal static class ListQsosCommand
{
    private const int CommentColumnWidth = 40;

    public static async Task<int> RunAsync(
        GrpcChannel channel,
        string[] args,
        bool jsonOutput = false,
        CancellationToken cancellationToken = default)
    {
        if (!TryParseArgs(args, out var request, out var displayOptions, out var error))
        {
            Console.Error.WriteLine(error);
            return 1;
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        using var call = client.ListQsos(request, cancellationToken: cancellationToken);

        if (jsonOutput)
        {
            var records = new List<Google.Protobuf.IMessage>();

            while (await call.ResponseStream.MoveNext(cancellationToken))
            {
                var qso = call.ResponseStream.Current.Qso;
                if (qso is not null)
                {
                    records.Add(qso);
                }
            }

            JsonOutput.PrintArray(records);
            return 0;
        }

        PrintHeader(displayOptions);

        var count = 0u;

        while (await call.ResponseStream.MoveNext(cancellationToken))
        {
            var qso = call.ResponseStream.Current.Qso;
            if (qso is null)
            {
                continue;
            }

            PrintRow(qso, displayOptions);
            count++;
        }

        Console.WriteLine();
        Console.WriteLine($"{count} QSO(s)");

        return 0;
    }

    private static void PrintHeader(ListDisplayOptions options)
    {
        var header = $"{"UTC",-20} {"Callsign",-12} {"Band",-8} {"Mode",-8}";

        if (options.ShowId)
        {
            header = $"{"UTC",-20} {"ID",-38} {"Callsign",-12} {"Band",-8} {"Mode",-8}";
        }

        if (options.ShowRst)
        {
            header += $" {"RST S",-6} {"RST R",-6}";
        }

        header += $" {"Freq",-10} {"Grid",-8}";

        if (options.ShowDuration)
        {
            header += $" {"Duration",-10}";
        }

        if (options.ShowComment)
        {
            header += $" {"Comment",-40}";
        }

        if (options.ShowDeletedAt)
        {
            header += $" {"Deleted",-22} {"Pending QRZ Delete",-18}";
        }

        Console.WriteLine(header);
        Console.WriteLine(new string('-', header.Length));
    }

    private static void PrintRow(QsoRecord qso, ListDisplayOptions options)
    {
        var utc = qso.UtcTimestamp?.ToDateTime().ToString("u") ?? "";
        var band = FormatBand(qso.Band);
        var mode = FormatMode(qso.Mode);
        var row = $"{utc,-20} {qso.WorkedCallsign,-12} {band,-8} {mode,-8}";

        if (options.ShowId)
        {
            row = $"{utc,-20} {qso.LocalId,-38} {qso.WorkedCallsign,-12} {band,-8} {mode,-8}";
        }

        if (options.ShowRst)
        {
            row += $" {FormatRst(qso.RstSent),-6} {FormatRst(qso.RstReceived),-6}";
        }

        ulong? freqHz = qso.HasFrequencyHz ? qso.FrequencyHz
#pragma warning disable CS0612
            : qso.HasFrequencyKhz ? qso.FrequencyKhz * 1000
#pragma warning restore CS0612
            : null;
        var freq = freqHz.HasValue ? FormatFrequencyMhz(freqHz.Value) : "";
        var grid = qso.HasWorkedGrid ? qso.WorkedGrid : "";
        row += $" {freq,-10} {grid,-8}";

        if (options.ShowDuration)
        {
            var duration = FormatDuration(qso) ?? "";
            row += $" {duration,-10}";
        }

        if (options.ShowComment)
        {
            row += $" {FormatCommentPreview(qso),-40}";
        }

        if (options.ShowDeletedAt)
        {
            var deletedAt = qso.DeletedAt is not null ? qso.DeletedAt.ToDateTime().ToString("u") : "";
            var pendingRemoteDelete = qso.PendingRemoteDelete ? "yes" : "";
            row += $" {deletedAt,-22} {pendingRemoteDelete,-18}";
        }

        Console.WriteLine(row);
    }

    internal static bool TryParseArgs(string[] args, out ListQsosRequest request, out ListDisplayOptions displayOptions, out string? error)
    {
        request = new ListQsosRequest { Limit = 20 };
        displayOptions = new ListDisplayOptions();
        error = null;

        var hasDeleted = false;
        var hasIncludeDeleted = false;

        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--show-id":
                    displayOptions.ShowId = true;
                    break;
                case "--show-rst":
                    displayOptions.ShowRst = true;
                    break;
                case "--show-comment":
                    displayOptions.ShowComment = true;
                    break;
                case "--show-duration":
                    displayOptions.ShowDuration = true;
                    break;
                case "--deleted":
                    hasDeleted = true;
                    break;
                case "--include-deleted":
                    hasIncludeDeleted = true;
                    break;
                case "--callsign" when i < args.Length - 1:
                    request.CallsignFilter = args[++i].ToUpperInvariant();
                    break;
                case "--callsign":
                    error = "Missing value for --callsign.";
                    return false;
                case "--band" when i < args.Length - 1:
                    try
                    {
                        request.BandFilter = EnumHelpers.ParseBand(args[++i]);
                    }
                    catch (ArgumentException ex)
                    {
                        error = ex.Message;
                        return false;
                    }

                    break;
                case "--band":
                    error = "Missing value for --band.";
                    return false;
                case "--mode" when i < args.Length - 1:
                    try
                    {
                        request.ModeFilter = EnumHelpers.ParseMode(args[++i]);
                    }
                    catch (ArgumentException ex)
                    {
                        error = ex.Message;
                        return false;
                    }

                    break;
                case "--mode":
                    error = "Missing value for --mode.";
                    return false;
                case "--after" when i < args.Length - 1:
                    var after = TimeParser.Parse(args[++i]);
                    if (after is null)
                    {
                        error = "Invalid --after value. Use relative (2.days, 3.hours) or absolute (2026-04-10).";
                        return false;
                    }

                    request.After = after;
                    break;
                case "--after":
                    error = "Missing value for --after.";
                    return false;
                case "--before" when i < args.Length - 1:
                    var before = TimeParser.Parse(args[++i]);
                    if (before is null)
                    {
                        error = "Invalid --before value. Use relative (2.days, 3.hours) or absolute (2026-04-10).";
                        return false;
                    }

                    request.Before = before;
                    break;
                case "--before":
                    error = "Missing value for --before.";
                    return false;
                case "--limit" when i < args.Length - 1:
                    var limitValue = args[++i];
                    if (!uint.TryParse(limitValue, out var limit))
                    {
                        error = $"Invalid value for --limit: {limitValue}";
                        return false;
                    }

                    request.Limit = limit;
                    break;
                case "--limit":
                    error = "Missing value for --limit.";
                    return false;
                default:
                    error = $"Unknown option: {args[i]}";
                    return false;
            }
        }

        if (hasDeleted && hasIncludeDeleted)
        {
            error = "--deleted and --include-deleted cannot be combined. Use --deleted for trash-only or --include-deleted for all records.";
            return false;
        }

        if (hasDeleted)
        {
            request.DeletedFilter = DeletedRecordsFilter.DeletedOnly;
            displayOptions.ShowDeletedAt = true;
        }
        else if (hasIncludeDeleted)
        {
            request.DeletedFilter = DeletedRecordsFilter.All;
            displayOptions.ShowDeletedAt = true;
        }

        return true;
    }

    internal static string FormatRst(RstReport? rst)
    {
        if (rst is null || (rst.Readability == 0 && rst.Strength == 0))
        {
            return rst?.Raw ?? "";
        }

        return rst.HasTone
            ? $"{rst.Readability}{rst.Strength}{rst.Tone}"
            : $"{rst.Readability}{rst.Strength}";
    }

    internal static string FormatCommentPreview(QsoRecord qso)
    {
        var comment = qso.HasComment ? qso.Comment : (qso.HasNotes ? qso.Notes : "");
        return TrimComment(comment);
    }

    internal static string? FormatDuration(QsoRecord qso)
    {
        var start = qso.UtcTimestamp?.ToDateTimeOffset();
        var end = qso.UtcEndTimestamp?.ToDateTimeOffset();
        return QsoDurationFormatter.Format(start, end);
    }

    internal static string TrimComment(string value)
    {
        var sanitized = value.ReplaceLineEndings(" ").Trim();
        if (sanitized.Length <= CommentColumnWidth)
        {
            return sanitized;
        }

        return $"{sanitized[..(CommentColumnWidth - 3)]}...";
    }

    private static string FormatFrequencyMhz(ulong hz)
    {
        ulong whole = hz / 1_000_000;
        ulong frac = hz % 1_000_000;
        string full = $"{whole}.{frac:000000}";
        int dotPos = full.IndexOf('.', StringComparison.Ordinal);
        int minLen = dotPos + 4;
        var trimmed = full.AsSpan().TrimEnd('0');
        int end = Math.Max(trimmed.Length, minLen);
        return full[..end];
    }
}

internal sealed class ListDisplayOptions
{
    public bool ShowComment { get; set; } = true;

    public bool ShowDuration { get; set; }

    public bool ShowId { get; set; }

    public bool ShowRst { get; set; }

    public bool ShowDeletedAt { get; set; }
}
