using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class UpdateQsoCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string localId, string[] args)
    {
        if (args.Length == 0 || args.Any(static a => a is "help" or "-?" or "--help"))
        {
            Console.WriteLine(CliHelpText.GetCommandHelp("update"));
            return args.Length == 0 ? 1 : 0;
        }

        var client = new LogbookService.LogbookServiceClient(channel);

        var getResponse = await client.GetQsoAsync(new GetQsoRequest { LocalId = localId });
        if (getResponse.Qso is not { } qso)
        {
            Console.Error.WriteLine($"QSO not found: {localId}");
            return 1;
        }

        var enrich = false;

        if (!TryApplyUpdates(args, qso, ref enrich, out var error))
        {
            Console.Error.WriteLine(error);
            return 1;
        }

        if (enrich)
        {
            await EnrichFromLookup(channel, qso);
        }

        var response = await client.UpdateQsoAsync(new UpdateQsoRequest { Qso = qso });

        if (!response.Success)
        {
            Console.Error.WriteLine($"Update failed: {response.Error}");
            return 1;
        }

        Console.WriteLine($"Updated QSO: {localId}");
        Console.WriteLine($"  {qso.WorkedCallsign} on {EnumHelpers.FormatBand(qso.Band)} {EnumHelpers.FormatMode(qso.Mode)}");

        if (qso.HasWorkedGrid)
        {
            Console.WriteLine($"  Grid: {qso.WorkedGrid}");
        }

        if (qso.HasWorkedCountry)
        {
            Console.WriteLine($"  Country: {qso.WorkedCountry}");
        }

        if (response.HasSyncError)
        {
            Console.WriteLine($"  QRZ sync: {response.SyncError}");
        }

        return 0;
    }

    internal static bool TryApplyUpdates(string[] args, QsoRecord qso, ref bool enrich, out string? error)
    {
        error = null;

        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--grid" when i < args.Length - 1:
                    qso.WorkedGrid = args[++i].ToUpperInvariant();
                    break;
                case "--grid":
                    error = "Missing value for --grid.";
                    return false;
                case "--country" when i < args.Length - 1:
                    qso.WorkedCountry = args[++i];
                    break;
                case "--country":
                    error = "Missing value for --country.";
                    return false;
                case "--state" when i < args.Length - 1:
                    qso.WorkedState = args[++i].ToUpperInvariant();
                    break;
                case "--state":
                    error = "Missing value for --state.";
                    return false;
                case "--freq" when i < args.Length - 1:
                    var freqValue = args[++i];
                    if (!ulong.TryParse(freqValue, out var freq))
                    {
                        error = $"Invalid value for --freq: {freqValue}";
                        return false;
                    }

                    qso.FrequencyKhz = freq;
                    break;
                case "--freq":
                    error = "Missing value for --freq.";
                    return false;
                case "--comment" when i < args.Length - 1:
                    qso.Comment = args[++i];
                    break;
                case "--comment":
                    error = "Missing value for --comment.";
                    return false;
                case "--band" when i < args.Length - 1:
                    try
                    {
                        qso.Band = EnumHelpers.ParseBand(args[++i]);
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
                        qso.Mode = EnumHelpers.ParseMode(args[++i]);
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
                case "--rst-sent" when i < args.Length - 1:
                    if (!LogQsoCommand.TryParseRst(args[++i], out var rstSent))
                    {
                        error = $"Invalid value for --rst-sent: {args[i]}. Expected 2 or 3 digits.";
                        return false;
                    }

                    qso.RstSent = rstSent;
                    break;
                case "--rst-sent":
                    error = "Missing value for --rst-sent.";
                    return false;
                case "--rst-rcvd" when i < args.Length - 1:
                    if (!LogQsoCommand.TryParseRst(args[++i], out var rstReceived))
                    {
                        error = $"Invalid value for --rst-rcvd: {args[i]}. Expected 2 or 3 digits.";
                        return false;
                    }

                    qso.RstReceived = rstReceived;
                    break;
                case "--rst-rcvd":
                    error = "Missing value for --rst-rcvd.";
                    return false;
                case "--at" when i < args.Length - 1:
                    var ts = TimeParser.Parse(args[++i]);
                    if (ts is null)
                    {
                        error = "Invalid --at value. Use relative (30.minutes, 2.hours) or absolute (2026-04-12T19:30:00Z).";
                        return false;
                    }

                    qso.UtcTimestamp = ts;
                    break;
                case "--at":
                    error = "Missing value for --at.";
                    return false;
                case "--enrich":
                    enrich = true;
                    break;
                default:
                    error = $"Unknown option: {args[i]}";
                    return false;
            }
        }

        return true;
    }

    private static async Task EnrichFromLookup(GrpcChannel channel, QsoRecord qso)
    {
        try
        {
            var lookupClient = new LookupService.LookupServiceClient(channel);
            var response = await lookupClient.LookupAsync(new LookupRequest
            {
                Callsign = qso.WorkedCallsign,
                SkipCache = false,
            });

            var result = response.Result;
            if (result is null || result.State != LookupState.Found || result.Record is null)
            {
                var reason = result?.State switch
                {
                    LookupState.NotFound => "callsign not found",
                    LookupState.Error => result.HasErrorMessage ? result.ErrorMessage : "lookup error",
                    _ => "no result",
                };
                Console.Error.WriteLine($"  \u26a0 Lookup skipped for {qso.WorkedCallsign}: {reason}");
                return;
            }

            var record = result.Record;

            if (record.HasGridSquare)
            {
                qso.WorkedGrid = record.GridSquare;
            }

            if (record.HasCountry)
            {
                qso.WorkedCountry = record.Country;
            }

            if (record.HasState)
            {
                qso.WorkedState = record.State;
            }

            if (record.DxccEntityId != 0)
            {
                qso.WorkedDxcc = record.DxccEntityId;
            }

            Console.WriteLine($"  Enriched from QRZ lookup");
        }
        catch (Grpc.Core.RpcException ex)
        {
            var detail = ex.Status.Detail;
            var message = string.IsNullOrEmpty(detail) ? ex.StatusCode.ToString() : detail;
            Console.Error.WriteLine($"  \u26a0 Lookup unavailable for {qso.WorkedCallsign}: {message}");
        }
    }
}
