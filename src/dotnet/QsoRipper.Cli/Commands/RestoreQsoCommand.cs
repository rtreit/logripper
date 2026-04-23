using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class RestoreQsoCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string localId)
    {
        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.RestoreQsoAsync(new RestoreQsoRequest { LocalId = localId });

        if (response.Success)
        {
            Console.WriteLine($"Restored QSO: {localId}");
        }
        else
        {
            Console.Error.WriteLine($"Failed to restore QSO: {response.Error}");
            return 1;
        }

        return 0;
    }
}
