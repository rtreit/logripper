using System.Diagnostics;
using Grpc.Net.Client;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class StreamLookupCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign, bool skipCache)
    {
        var client = new LookupService.LookupServiceClient(channel);
        using var call = client.StreamLookup(new StreamLookupRequest
        {
            Callsign = callsign,
            SkipCache = skipCache,
        });

        Console.WriteLine($"Streaming lookup for {callsign}...");
        Console.WriteLine();

        var stopwatch = Stopwatch.StartNew();
        LookupResult? lastResult = null;

        while (await call.ResponseStream.MoveNext(CancellationToken.None))
        {
            var update = call.ResponseStream.Current.Result ?? new LookupResult();
            var state = update.State;
            var elapsed = stopwatch.ElapsedMilliseconds;

            Console.WriteLine($"[{elapsed,6} ms]  {state}");

            if (update.HasErrorMessage)
            {
                Console.WriteLine($"           Error: {update.ErrorMessage}");
            }

            lastResult = update;
        }

        if (lastResult?.Record is { } record)
        {
            Console.WriteLine();
            LookupCommand.PrintRecord(record);
        }

        var finalState = lastResult?.State ?? LookupState.Unspecified;
        return finalState == LookupState.Found ? 0 : 1;
    }
}
