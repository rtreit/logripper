using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class SyncCommand
{
    public static async Task<int> RunAsync(
        GrpcChannel channel,
        bool force,
        CancellationToken cancellationToken = default)
    {
        var client = new LogbookService.LogbookServiceClient(channel);
        var request = new SyncWithQrzRequest { FullSync = force };
        using var call = client.SyncWithQrz(request, cancellationToken: cancellationToken);

        SyncWithQrzResponse? last = null;

        while (await call.ResponseStream.MoveNext(cancellationToken))
        {
            var update = call.ResponseStream.Current;

            if (update.HasCurrentAction)
            {
                Console.WriteLine(update.CurrentAction);
            }

            last = update;
        }

        if (last is null)
        {
            Console.Error.WriteLine("No response received from engine.");
            return 1;
        }

        if (last.HasError)
        {
            Console.Error.WriteLine($"Sync failed: {last.Error}");
            return 1;
        }

        Console.WriteLine();
        Console.WriteLine($"Downloaded:  {last.DownloadedRecords}");
        Console.WriteLine($"Uploaded:    {last.UploadedRecords}");
        Console.WriteLine($"Conflicts:   {last.ConflictRecords}");
        Console.WriteLine($"Total:       {last.TotalRecords}");

        return 0;
    }
}
