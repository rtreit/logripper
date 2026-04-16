using Grpc.Net.Client;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class RigStatusCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, bool jsonOutput = false)
    {
        var client = new RigControlService.RigControlServiceClient(channel);

        if (jsonOutput)
        {
            return await RunJsonAsync(client);
        }

        var statusResponse = await client.GetRigStatusAsync(new GetRigStatusRequest());
        var status = statusResponse.Status;

        Console.WriteLine($"Rig: {FormatStatus(status)}");

        if (statusResponse.HasEndpoint)
        {
            Console.WriteLine($"  Endpoint: {statusResponse.Endpoint}");
        }

        if (statusResponse.HasErrorMessage)
        {
            Console.WriteLine($"  Error: {statusResponse.ErrorMessage}");
        }

        if (status != RigConnectionStatus.Connected)
        {
            return 0;
        }

        var snapshotResponse = await client.GetRigSnapshotAsync(new GetRigSnapshotRequest());
        if (snapshotResponse.Snapshot is { } snapshot)
        {
            var freq = snapshot.FrequencyHz > 0 ? $"{snapshot.FrequencyHz / 1_000_000.0:F3} MHz" : "unknown";
            var band = snapshot.Band != Band.Unspecified ? EnumHelpers.FormatBand(snapshot.Band) : "unknown";
            var mode = snapshot.HasRawMode ? snapshot.RawMode : "unknown";

            Console.WriteLine($"  Frequency: {freq} ({band})");
            Console.WriteLine($"  Mode: {mode}");

            if (snapshot.HasSubmode)
            {
                Console.WriteLine($"  Submode: {snapshot.Submode}");
            }
        }

        return 0;
    }

    private static async Task<int> RunJsonAsync(RigControlService.RigControlServiceClient client)
    {
        var response = await client.GetRigSnapshotAsync(new GetRigSnapshotRequest());

        if (response.Snapshot is not { } snapshot)
        {
            Console.WriteLine("{\"status\":\"disabled\"}");
            return 0;
        }

        var statusStr = snapshot.Status switch
        {
            RigConnectionStatus.Connected => "connected",
            RigConnectionStatus.Error => "error",
            RigConnectionStatus.Disabled => "disabled",
            _ => "disconnected",
        };

        if (snapshot.Status != RigConnectionStatus.Connected)
        {
            Console.WriteLine($"{{\"status\":\"{statusStr}\"}}");
            return 0;
        }

        var freqMhz = snapshot.FrequencyHz > 0
            ? FormattableString.Invariant($"{snapshot.FrequencyHz / 1_000_000.0:F3}")
            : "";
        var freqDisplay = freqMhz.Length > 0 ? $"{freqMhz} MHz" : "";
        var band = snapshot.Band != Band.Unspecified ? EnumHelpers.FormatBand(snapshot.Band) : "";
        var mode = snapshot.Mode != Mode.Unspecified ? EnumHelpers.FormatMode(snapshot.Mode) : "";
        var rawMode = snapshot.HasRawMode ? snapshot.RawMode : "";

        var parts = new List<string> { "\"status\":\"connected\"" };
        if (freqMhz.Length > 0)
        {
            parts.Add($"\"frequencyHz\":{snapshot.FrequencyHz}");
            parts.Add($"\"frequencyDisplay\":\"{freqDisplay}\"");
            parts.Add($"\"frequencyMhz\":\"{freqMhz}\"");
        }

        if (band.Length > 0)
        {
            parts.Add($"\"band\":\"{band}\"");
        }

        if (mode.Length > 0)
        {
            parts.Add($"\"mode\":\"{mode}\"");
        }

        if (rawMode.Length > 0)
        {
            parts.Add($"\"rawMode\":\"{rawMode}\"");
        }

        Console.WriteLine("{" + string.Join(",", parts) + "}");
        return 0;
    }

    private static string FormatStatus(RigConnectionStatus status)
    {
        return status switch
        {
            RigConnectionStatus.Connected => "\u2705 Connected",
            RigConnectionStatus.Disconnected => "\u274c Disconnected",
            RigConnectionStatus.Error => "\u26a0 Error",
            RigConnectionStatus.Disabled => "\u2b55 Disabled",
            _ => "Unknown",
        };
    }
}
