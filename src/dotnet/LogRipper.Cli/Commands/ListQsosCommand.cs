using Grpc.Core;
using Grpc.Net.Client;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class ListQsosCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string[] args)
    {
        var request = new ListQsosRequest { Limit = 20 };

        for (var i = 0; i < args.Length - 1; i++)
        {
            switch (args[i])
            {
                case "--callsign":
                    request.CallsignFilter = args[++i].ToUpperInvariant();
                    break;
                case "--band":
                    try
                    {
                        request.BandFilter = EnumHelpers.ParseBand(args[++i]);
                    }
                    catch (ArgumentException ex)
                    {
                        Console.Error.WriteLine(ex.Message);
                        return 1;
                    }

                    break;
                case "--mode":
                    try
                    {
                        request.ModeFilter = EnumHelpers.ParseMode(args[++i]);
                    }
                    catch (ArgumentException ex)
                    {
                        Console.Error.WriteLine(ex.Message);
                        return 1;
                    }

                    break;
                case "--limit":
                    if (uint.TryParse(args[++i], out var limit))
                    {
                        request.Limit = limit;
                    }

                    break;
            }
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        using var call = client.ListQsos(request);

        Console.WriteLine($"{"UTC",-20} {"Callsign",-12} {"Band",-8} {"Mode",-8} {"RST S",-6} {"RST R",-6}");
        Console.WriteLine(new string('-', 62));

        var count = 0u;

        while (await call.ResponseStream.MoveNext(CancellationToken.None))
        {
            var qso = call.ResponseStream.Current.Qso;
            if (qso is null)
            {
                continue;
            }

            var utc = qso.UtcTimestamp?.ToDateTime().ToString("u") ?? "";
            var rstS = qso.RstSent is not null ? $"{qso.RstSent.Readability}{qso.RstSent.Strength}" : "";
            var rstR = qso.RstReceived is not null ? $"{qso.RstReceived.Readability}{qso.RstReceived.Strength}" : "";

            Console.WriteLine($"{utc,-20} {qso.WorkedCallsign,-12} {qso.Band,-8} {qso.Mode,-8} {rstS,-6} {rstR,-6}");
            count++;
        }

        Console.WriteLine();
        Console.WriteLine($"{count} QSO(s)");

        return 0;
    }
}
