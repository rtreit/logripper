using Grpc.Net.Client;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class LookupCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string callsign, bool skipCache)
    {
        var client = new LookupService.LookupServiceClient(channel);
        var response = await client.LookupAsync(new LookupRequest
        {
            Callsign = callsign,
            SkipCache = skipCache,
        });

        var state = (LookupState)response.State;
        Console.WriteLine($"State:            {state}");
        Console.WriteLine($"Queried:          {response.QueriedCallsign}");
        Console.WriteLine($"Cache hit:        {response.CacheHit}");
        Console.WriteLine($"Latency:          {response.LookupLatencyMs} ms");

        if (response.HasErrorMessage)
        {
            Console.WriteLine($"Error:            {response.ErrorMessage}");
        }

        if (response.Record is { } record)
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
