using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class DeleteQsoCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string localId)
    {
        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.DeleteQsoAsync(new DeleteQsoRequest { LocalId = localId });

        if (response.Success)
        {
            Console.WriteLine($"Deleted QSO: {localId}");
        }
        else
        {
            Console.Error.WriteLine($"Failed to delete QSO: {response.Error}");
            return 1;
        }

        return 0;
    }
}
