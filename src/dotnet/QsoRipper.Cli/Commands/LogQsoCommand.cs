using Google.Protobuf.WellKnownTypes;
using Grpc.Net.Client;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class LogQsoCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign, string[] args)
    {
        if (!TryBuildQso(callsign, args, out var qso, out var noEnrich, out var fromRig, out var error))
        {
            if (error is not null)
            {
                Console.Error.WriteLine(error);
            }
            else
            {
                Console.WriteLine(CliHelpText.GetCommandHelp("log"));
            }

            return error is not null ? 1 : 0;
        }

        var requestQso = qso!;

        if (fromRig)
        {
            if (!await ApplyRigSnapshot(channel, requestQso))
            {
                return 1;
            }
        }

        if (requestQso.Band == Band.Unspecified || requestQso.Mode == Mode.Unspecified)
        {
            Console.Error.WriteLine("Band and mode are required. Provide them as positional args, with --band/--mode flags, or use --from-rig.");
            return 1;
        }

        ApplyDefaultRst(requestQso);

        if (!noEnrich)
        {
            await EnrichFromLookup(channel, requestQso);
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.LogQsoAsync(new LogQsoRequest { Qso = requestQso });

        Console.WriteLine($"QSO logged: {response.LocalId}");
        Console.WriteLine($"  {requestQso.WorkedCallsign} on {EnumHelpers.FormatBand(requestQso.Band)} {EnumHelpers.FormatMode(requestQso.Mode)} at {requestQso.UtcTimestamp!.ToDateTime():u}");

        if (requestQso.HasWorkedGrid)
        {
            Console.WriteLine($"  Grid: {requestQso.WorkedGrid}");
        }

        if (requestQso.HasWorkedCountry)
        {
            Console.WriteLine($"  Country: {requestQso.WorkedCountry}");
        }

        if (requestQso.HasComment)
        {
            Console.WriteLine($"  Comment: {requestQso.Comment}");
        }

        if (requestQso.HasNotes)
        {
            Console.WriteLine($"  Notes: {requestQso.Notes}");
        }

        if (response.HasSyncError)
        {
            Console.WriteLine($"  QRZ sync: {response.SyncError}");
        }

        return 0;
    }

    internal static bool TryBuildQso(string callsign, string[] args, out QsoRecord? qso, out bool noEnrich, out string? error)
    {
        return TryBuildQso(callsign, args, out qso, out noEnrich, out _, out error);
    }

    internal static bool TryBuildQso(string callsign, string[] args, out QsoRecord? qso, out bool noEnrich, out bool fromRig, out string? error)
    {
        qso = null;
        noEnrich = false;
        fromRig = args.Any(static a => a is "--from-rig");
        error = null;

        if (args.Any(static a => a is "help" or "-?" or "--help"))
        {
            error = null;
            return false;
        }

        // With --from-rig, band and mode are optional positional args.
        // Without --from-rig, the first two args must be band and mode.
        Band? band = null;
        Mode? mode = null;
        var optionalArgsStart = 0;

        if (args.Length >= 2 && !args[0].StartsWith('-') && !args[1].StartsWith('-'))
        {
            try
            {
                band = EnumHelpers.ParseBand(args[0]);
                mode = EnumHelpers.ParseMode(args[1]);
                optionalArgsStart = 2;
            }
            catch (ArgumentException ex)
            {
                if (!fromRig)
                {
                    error = ex.Message;
                    return false;
                }
                // With --from-rig, treat unparseable positional args as remaining args
            }
        }
        else if (!fromRig)
        {
            error = "Usage: log <callsign> <band> <mode> [--station call] [--at time] [--rst-sent 59] [--rst-rcvd 59] [--freq khz] [--comment text] [--notes text] [--no-enrich] [--from-rig]";
            return false;
        }

        qso = new QsoRecord
        {
            WorkedCallsign = callsign.ToUpperInvariant(),
            UtcTimestamp = Timestamp.FromDateTime(DateTime.UtcNow),
        };

        if (band.HasValue)
        {
            qso.Band = band.Value;
        }

        if (mode.HasValue)
        {
            qso.Mode = mode.Value;
        }

        return TryApplyOptionalArgs(args, qso, optionalArgsStart, ref noEnrich, out error);
    }

    internal static RstReport DefaultRst(Mode mode)
    {
        return IsPhoneMode(mode)
            ? new RstReport { Readability = 5, Strength = 9 }
            : new RstReport { Readability = 5, Strength = 9, Tone = 9 };
    }

    internal static bool IsPhoneMode(Mode mode)
    {
        return mode is Mode.Ssb or Mode.Am or Mode.Fm or Mode.Digitalvoice or Mode.Voi;
    }

    internal static void ApplyDefaultRst(QsoRecord qso)
    {
        var defaultRst = DefaultRst(qso.Mode);
        qso.RstSent ??= defaultRst.Clone();
        qso.RstReceived ??= defaultRst.Clone();
    }

    internal static bool TryApplyOptionalArgs(string[] args, QsoRecord qso, ref bool noEnrich, out string? error)
    {
        return TryApplyOptionalArgs(args, qso, 2, ref noEnrich, out error);
    }

    internal static bool TryApplyOptionalArgs(string[] args, QsoRecord qso, int startIndex, ref bool noEnrich, out string? error)
    {
        error = null;

        for (var i = startIndex; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--station" when i < args.Length - 1:
                    qso.StationCallsign = args[++i].ToUpperInvariant();
                    break;
                case "--station":
                    error = "Missing value for --station.";
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
                case "--rst-sent" when i < args.Length - 1:
                    if (!TryParseRst(args[++i], out var rstSent))
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
                    if (!TryParseRst(args[++i], out var rstReceived))
                    {
                        error = $"Invalid value for --rst-rcvd: {args[i]}. Expected 2 or 3 digits.";
                        return false;
                    }

                    qso.RstReceived = rstReceived;
                    break;
                case "--rst-rcvd":
                    error = "Missing value for --rst-rcvd.";
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
                case "--notes" when i < args.Length - 1:
                    qso.Notes = args[++i];
                    break;
                case "--notes":
                    error = "Missing value for --notes.";
                    return false;
                case "--no-enrich":
                    noEnrich = true;
                    break;
                case "--from-rig":
                    // Already handled in TryBuildQso
                    break;
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
                default:
                    error = $"Unknown option: {args[i]}";
                    return false;
            }
        }

        return true;
    }

    internal static bool TryParseRst(string value, out RstReport report)
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

    private static async Task<bool> ApplyRigSnapshot(GrpcChannel channel, QsoRecord qso)
    {
        try
        {
            var rigClient = new RigControlService.RigControlServiceClient(channel);
            var response = await rigClient.GetRigSnapshotAsync(new GetRigSnapshotRequest());

            if (response.Snapshot is not { } snapshot
                || snapshot.Status != RigConnectionStatus.Connected)
            {
                var errorMsg = response.Snapshot?.ErrorMessage ?? "rig unavailable";
                Console.Error.WriteLine($"  \u26a0 Rig read failed: {errorMsg}");
                return false;
            }

            // Fill band, mode, frequency, submode from rig if not explicitly set
            if (qso.Band == Band.Unspecified && snapshot.Band != Band.Unspecified)
            {
                qso.Band = snapshot.Band;
            }

            if (qso.Mode == Mode.Unspecified && snapshot.Mode != Mode.Unspecified)
            {
                qso.Mode = snapshot.Mode;
            }

            if (qso.FrequencyKhz == 0 && snapshot.FrequencyHz > 0)
            {
                qso.FrequencyKhz = (snapshot.FrequencyHz + 500) / 1000;
            }

            if (!qso.HasSubmode && snapshot.HasSubmode)
            {
                qso.Submode = snapshot.Submode;
            }

            var freq = snapshot.FrequencyHz > 0 ? $"{snapshot.FrequencyHz / 1_000_000.0:F3} MHz" : "unknown";
            var rawMode = snapshot.HasRawMode ? snapshot.RawMode : "unknown";
            Console.WriteLine($"  Rig: {freq} {rawMode}");

            return true;
        }
        catch (Grpc.Core.RpcException ex)
        {
            var detail = ex.Status.Detail;
            var message = string.IsNullOrEmpty(detail) ? ex.StatusCode.ToString() : detail;
            Console.Error.WriteLine($"  \u26a0 Rig control unavailable: {message}");
            return false;
        }
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

            if (record.HasGridSquare && !qso.HasWorkedGrid)
            {
                qso.WorkedGrid = record.GridSquare;
            }

            if (record.HasCountry && !qso.HasWorkedCountry)
            {
                qso.WorkedCountry = record.Country;
            }

            if (record.HasState && !qso.HasWorkedState)
            {
                qso.WorkedState = record.State;
            }

            if (record.DxccEntityId != 0 && qso.WorkedDxcc == 0)
            {
                qso.WorkedDxcc = record.DxccEntityId;
            }
        }
        catch (Grpc.Core.RpcException ex)
        {
            var detail = ex.Status.Detail;
            var message = string.IsNullOrEmpty(detail) ? ex.StatusCode.ToString() : detail;
            Console.Error.WriteLine($"  \u26a0 Lookup unavailable for {qso.WorkedCallsign}: {message}");
        }
    }
}
