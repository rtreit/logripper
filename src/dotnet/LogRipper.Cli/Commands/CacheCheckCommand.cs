using Grpc.Net.Client;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class CacheCheckCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign)
    {
        var client = new LookupService.LookupServiceClient(channel);
        var response = await client.GetCachedCallsignAsync(new CachedCallsignRequest
        {
            Callsign = callsign,
        });

        var state = response.State;

        if (response.CacheHit && response.Record is not null)
        {
            Console.WriteLine($"{callsign} is cached.");
            Console.WriteLine();
            LookupCommand.PrintRecord(response.Record);
            return 0;
        }

        Console.WriteLine($"{callsign} is not in the cache. (state: {state})");
        return 1;
    }
}
