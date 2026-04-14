using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Domain;
using QsoRipper.Services;
using static QsoRipper.Cli.EnumHelpers;

namespace QsoRipper.Cli.Commands;

internal static class ListQsosCommand
{
    private const int CommentColumnWidth = 40;

    public static async Task<int> RunAsync(GrpcChannel channel, string[] args, bool jsonOutput = false)
    {
        if (!TryParseArgs(args, out var request, out var displayOptions, out var error))
        {
            Console.Error.WriteLine(error);
            return 1;
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        using var call = client.ListQsos(request);

        if (jsonOutput)
        {
            var records = new List<Google.Protobuf.IMessage>();

            while (await call.ResponseStream.MoveNext(CancellationToken.None))
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

        while (await call.ResponseStream.MoveNext(CancellationToken.None))
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

        if (options.ShowComment)
        {
            header += $" {"Comment",-40}";
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

        var freq = qso.HasFrequencyKhz ? $"{qso.FrequencyKhz / 1000.0:F3}" : "";
        var grid = qso.HasWorkedGrid ? qso.WorkedGrid : "";
        row += $" {freq,-10} {grid,-8}";

        if (options.ShowComment)
        {
            row += $" {FormatCommentPreview(qso),-40}";
        }

        Console.WriteLine(row);
    }

    internal static bool TryParseArgs(string[] args, out ListQsosRequest request, out ListDisplayOptions displayOptions, out string? error)
    {
        request = new ListQsosRequest { Limit = 20 };
        displayOptions = new ListDisplayOptions();
        error = null;

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

    internal static string TrimComment(string value)
    {
        var sanitized = value.ReplaceLineEndings(" ").Trim();
        if (sanitized.Length <= CommentColumnWidth)
        {
            return sanitized;
        }

        return $"{sanitized[..(CommentColumnWidth - 3)]}...";
    }
}

internal sealed class ListDisplayOptions
{
    public bool ShowComment { get; set; } = true;

    public bool ShowId { get; set; }

    public bool ShowRst { get; set; }
}
