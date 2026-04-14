using Google.Protobuf.WellKnownTypes;
using Grpc.Net.Client;
using QsoRipper.Domain;
using QsoRipper.Services;
using System.Globalization;

namespace QsoRipper.Cli.Commands;

internal static class SpaceWeatherCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, bool refresh = false, bool jsonOutput = false)
    {
        var client = new SpaceWeatherService.SpaceWeatherServiceClient(channel);

        var snapshot = refresh
            ? (await client.RefreshSpaceWeatherAsync(new RefreshSpaceWeatherRequest())).Snapshot
            : (await client.GetCurrentSpaceWeatherAsync(new GetCurrentSpaceWeatherRequest())).Snapshot;

        return HandleSnapshot(snapshot, jsonOutput);
    }

    internal static int HandleSnapshot(SpaceWeatherSnapshot? snapshot, bool jsonOutput)
    {
        if (snapshot is null)
        {
            Console.Error.WriteLine("Space weather snapshot unavailable.");
            return 1;
        }

        if (jsonOutput)
        {
            JsonOutput.Print(snapshot);
        }
        else
        {
            PrintSnapshot(snapshot);
        }

        return snapshot.Status == SpaceWeatherStatus.Error ? 1 : 0;
    }

    internal static void PrintSnapshot(SpaceWeatherSnapshot snapshot)
    {
        Console.WriteLine($"Status:           {FormatStatus(snapshot.Status)}");
        Console.WriteLine($"Observed at:      {FormatTimestamp(snapshot.ObservedAt)}");
        Console.WriteLine($"Fetched at:       {FormatTimestamp(snapshot.FetchedAt)}");
        Console.WriteLine($"Planetary K:      {FormatDouble(snapshot.HasPlanetaryKIndex, snapshot.PlanetaryKIndex)}");
        Console.WriteLine($"Planetary A:      {FormatUInt32(snapshot.HasPlanetaryAIndex, snapshot.PlanetaryAIndex)}");
        Console.WriteLine($"Solar flux:       {FormatDouble(snapshot.HasSolarFluxIndex, snapshot.SolarFluxIndex)}");
        Console.WriteLine($"Sunspot number:   {FormatUInt32(snapshot.HasSunspotNumber, snapshot.SunspotNumber)}");
        Console.WriteLine($"Storm scale:      {FormatUInt32(snapshot.HasGeomagneticStormScale, snapshot.GeomagneticStormScale, "G")}");
        Console.WriteLine($"Source:           {FormatString(snapshot.SourceName)}");

        if (snapshot.ErrorMessage is not null)
        {
            Console.WriteLine($"Error:            {snapshot.ErrorMessage}");
        }
    }

    private static string FormatStatus(SpaceWeatherStatus status)
    {
        return status switch
        {
            SpaceWeatherStatus.Current => "current",
            SpaceWeatherStatus.Stale => "stale",
            SpaceWeatherStatus.Error => "error",
            _ => "unspecified"
        };
    }

    private static string FormatTimestamp(Timestamp? timestamp)
    {
        return timestamp is null ? "(unavailable)" : timestamp.ToDateTime().ToUniversalTime().ToString("u");
    }

    private static string FormatDouble(bool hasValue, double value)
    {
        return hasValue ? value.ToString("0.##", CultureInfo.InvariantCulture) : "(unavailable)";
    }

    private static string FormatUInt32(bool hasValue, uint value, string? prefix = null)
    {
        if (!hasValue)
        {
            return "(unavailable)";
        }

        return prefix is null ? value.ToString(CultureInfo.InvariantCulture) : $"{prefix}{value}";
    }

    private static string FormatString(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? "(unavailable)" : value;
    }
}
