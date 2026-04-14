using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class TestLogbookCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string[] args)
    {
        string? apiKey = null;

        for (var i = 0; i < args.Length; i++)
        {
            if (args[i] is "--api-key" && i < args.Length - 1)
            {
                apiKey = args[++i];
            }
        }

        var client = new SetupService.SetupServiceClient(channel);
        var request = new TestQrzLogbookCredentialsRequest();

        if (apiKey is not null)
        {
            request.ApiKey = apiKey;
        }

        var response = await client.TestQrzLogbookCredentialsAsync(request);

        if (!response.Success)
        {
            Console.Error.WriteLine($"Logbook test failed: {response.ErrorMessage}");
            return 1;
        }

        Console.WriteLine("Logbook credentials OK");

        if (response.HasLogbookOwner)
        {
            Console.WriteLine($"Owner:     {response.LogbookOwner}");
        }

        if (response.HasQsoCount)
        {
            Console.WriteLine($"QSO count: {response.QsoCount}");
        }

        return 0;
    }
}
