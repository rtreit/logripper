using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class CacheCheckCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign, bool jsonOutput = false)
    {
        var client = new LookupService.LookupServiceClient(channel);
        var response = await client.GetCachedCallsignAsync(new GetCachedCallsignRequest
        {
            Callsign = callsign,
        });
        var result = response.Result ?? new LookupResult();

        if (jsonOutput)
        {
            JsonOutput.Print(response);
            return (result.CacheHit && result.Record is not null) ? 0 : 1;
        }

        var state = result.State;

        if (result.CacheHit && result.Record is not null)
        {
            Console.WriteLine($"{callsign} is cached.");
            Console.WriteLine();
            LookupCommand.PrintRecord(result.Record);
            return 0;
        }

        Console.WriteLine($"{callsign} is not in the cache. (state: {state})");
        return 1;
    }
}
