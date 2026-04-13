using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class SetupCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, bool jsonOutput = false)
    {
        var client = new SetupService.SetupServiceClient(channel);
        var response = await client.GetSetupStatusAsync(new GetSetupStatusRequest());
        var status = response.Status;

        if (status is null)
        {
            Console.WriteLine("Setup status unavailable.");
            return 1;
        }

        if (jsonOutput)
        {
            JsonOutput.Print(response);
            return status.SetupComplete ? 0 : 1;
        }

        Console.WriteLine($"Setup complete:    {status.SetupComplete}");
        Console.WriteLine($"Config path:       {status.ConfigPath}");
        Console.WriteLine($"QRZ username:      {status.QrzXmlUsername ?? "(not set)"}");
        Console.WriteLine($"Station profile:   {status.HasStationProfile}");
        Console.WriteLine($"Storage backend:   {status.StorageBackend}");

        return status.SetupComplete ? 0 : 1;
    }
}
