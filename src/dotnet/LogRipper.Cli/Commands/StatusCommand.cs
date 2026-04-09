using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

public static class StatusCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel)
    {
        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.GetSyncStatusAsync(new SyncStatusRequest());

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
