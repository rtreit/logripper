using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class SyncStatusCommand
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
        Console.WriteLine($"Auto-sync:        {(response.AutoSyncEnabled ? "enabled" : "disabled")}");
        Console.WriteLine($"Syncing now:      {(response.IsSyncing ? "yes" : "no")}");

        if (response.LastSync is not null)
        {
            Console.WriteLine($"Last sync:        {response.LastSync.ToDateTime():u}");
        }
        else
        {
            Console.WriteLine("Last sync:        never");
        }

        if (response.NextSync is not null)
        {
            Console.WriteLine($"Next sync:        {response.NextSync.ToDateTime():u}");
        }

        if (response.HasQrzLogbookOwner)
        {
            Console.WriteLine($"QRZ owner:        {response.QrzLogbookOwner}");
        }

        if (response.HasLastSyncError)
        {
            Console.WriteLine($"Last error:       {response.LastSyncError}");
        }

        return 0;
    }
}
