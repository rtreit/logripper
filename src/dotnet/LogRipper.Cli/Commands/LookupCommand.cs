using Grpc.Net.Client;
using LogRipper.Cli;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class LookupCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign, bool skipCache, bool jsonOutput = false)
    {
        var client = new LookupService.LookupServiceClient(channel);
        var response = await client.LookupAsync(new LookupRequest
        {
            Callsign = callsign,
            SkipCache = skipCache,
        });
        var result = response.Result ?? new LookupResult();

        if (jsonOutput)
        {
            JsonOutput.Print(response);
            return (LookupState)result.State == LookupState.Found ? 0 : 1;
        }

        var state = (LookupState)result.State;
        Console.WriteLine($"State:            {state}");
        Console.WriteLine($"Queried:          {result.QueriedCallsign}");
        Console.WriteLine($"Cache hit:        {result.CacheHit}");
        Console.WriteLine($"Latency:          {result.LookupLatencyMs} ms");

        if (result.HasErrorMessage)
        {
            Console.WriteLine($"Error:            {result.ErrorMessage}");
        }

        if (result.Record is { } record)
        {
            PrintRecord(record);
        }

        return state == LookupState.Found ? 0 : 1;
    }

    internal static void PrintRecord(CallsignRecord record)
    {
        Console.WriteLine();
        Console.WriteLine($"Callsign:         {record.Callsign}");

        var name = FormatName(record);
        if (name is not null)
        {
            Console.WriteLine($"Name:             {name}");
        }

        if (record.HasCountry)
        {
            Console.WriteLine($"Country:          {record.Country}");
        }

        if (record.HasState)
        {
            Console.WriteLine($"State:            {record.State}");
        }

        if (record.HasGridSquare)
        {
            Console.WriteLine($"Grid:             {record.GridSquare}");
        }

        if (record.HasLatitude && record.HasLongitude)
        {
            Console.WriteLine($"Coordinates:      {record.Latitude:F4}, {record.Longitude:F4}");
        }

        if (record.HasLicenseClass)
        {
            Console.WriteLine($"License class:    {record.LicenseClass}");
        }

        var qsl = (QslPreference)record.Lotw;
        if (qsl != QslPreference.Unknown)
        {
            Console.WriteLine($"LoTW:             {qsl}");
        }
    }

    private static string? FormatName(CallsignRecord record)
    {
        if (record.HasFormattedName)
        {
            return record.FormattedName;
        }

        var first = record.FirstName;
        var last = record.LastName;

        if (string.IsNullOrEmpty(first) && string.IsNullOrEmpty(last))
        {
            return null;
        }

        return $"{first} {last}".Trim();
    }
}
