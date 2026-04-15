using Grpc.Net.Client;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class RigStatusCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel)
    {
        var client = new RigControlService.RigControlServiceClient(channel);

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
