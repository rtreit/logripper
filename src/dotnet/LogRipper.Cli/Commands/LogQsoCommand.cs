using Google.Protobuf.WellKnownTypes;
using Grpc.Net.Client;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class LogQsoCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign, string[] args)
    {
        if (args.Length < 2 || args.Any(a => a is "help" or "-?" or "--help"))
        {
            Console.Error.WriteLine("Usage: log <callsign> <band> <mode> [--station call] [--rst-sent 59] [--rst-rcvd 59] [--freq khz]");
            return 1;
        }

        Band band;
        Mode mode;

        try
        {
            band = EnumHelpers.ParseBand(args[0]);
            mode = EnumHelpers.ParseMode(args[1]);
        }
        catch (ArgumentException ex)
        {
            Console.Error.WriteLine(ex.Message);
            return 1;
        }

        var qso = new QsoRecord
        {
            WorkedCallsign = callsign,
            Band = band,
            Mode = mode,
            UtcTimestamp = Timestamp.FromDateTime(DateTime.UtcNow),
        };

        ParseOptionalArgs(args, qso);

        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.LogQsoAsync(new LogQsoRequest { Qso = qso });

        Console.WriteLine($"QSO logged: {response.LocalId}");
        Console.WriteLine($"  {callsign} on {band} {mode} at {DateTime.UtcNow:u}");

        if (response.HasSyncError)
        {
            Console.WriteLine($"  QRZ sync: {response.SyncError}");
        }

        return 0;
    }

    private static void ParseOptionalArgs(string[] args, QsoRecord qso)
    {
        for (var i = 2; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--station" when i < args.Length - 1:
                    qso.StationCallsign = args[++i].ToUpperInvariant();
                    break;
                case "--rst-sent" when i < args.Length - 1:
                    qso.RstSent = ParseRst(args[++i]);
                    break;
                case "--rst-rcvd" when i < args.Length - 1:
                    qso.RstReceived = ParseRst(args[++i]);
                    break;
                case "--freq" when i < args.Length - 1:
                    if (ulong.TryParse(args[++i], out var freq))
                    {
                        qso.FrequencyKhz = freq;
                    }

                    break;
            }
        }
    }

    private static RstReport ParseRst(string value)
    {
        var report = new RstReport();

        if (value.Length >= 2)
        {
            report.Readability = (uint)(value[0] - '0');
            report.Strength = (uint)(value[1] - '0');
        }

        if (value.Length >= 3)
        {
            report.Tone = (uint)(value[2] - '0');
        }

        return report;
    }
}
