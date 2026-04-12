using Grpc.Net.Client;
using LogRipper.Cli;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class StatusCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, bool jsonOutput = false)
    {
        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.GetSyncStatusAsync(new GetSyncStatusRequest());

        if (jsonOutput)
        {
            JsonOutput.Print(response);
            return 0;
        }

        Console.WriteLine($"Local QSOs:       {response.LocalQsoCount}");
        Console.WriteLine($"QRZ QSOs:         {response.QrzQsoCount}");
        Console.WriteLine($"Pending upload:   {response.PendingUpload}");

        if (response.LastSync is not null)
        {
            Console.WriteLine($"Last sync:        {response.LastSync.ToDateTime():u}");
        }
        else
        {
            Console.WriteLine("Last sync:        never");
        }

        if (response.HasQrzLogbookOwner)
        {
            Console.WriteLine($"QRZ owner:        {response.QrzLogbookOwner}");
        }

        return 0;
    }
}
