using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class StatusCommand
{
    public static async Task<int> RunAsync(
        GrpcChannel channel,
        string endpoint,
        EngineTargetProfile engineProfile,
        bool jsonOutput = false)
    {
        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.GetSyncStatusAsync(new GetSyncStatusRequest());

        if (jsonOutput)
        {
            JsonOutput.Print(response);
            return 0;
        }

        var engineClient = new EngineService.EngineServiceClient(channel);
        EngineInfo? engineInfo = null;
        try
        {
            engineInfo = (await engineClient.GetEngineInfoAsync(new GetEngineInfoRequest())).Engine;
        }
        catch (Grpc.Core.RpcException ex) when (ex.StatusCode == Grpc.Core.StatusCode.Unimplemented)
        {
        }

        Console.WriteLine(
            $"Engine:           {(engineInfo is null ? engineProfile.DisplayName : engineInfo.DisplayName)}{(string.IsNullOrWhiteSpace(engineInfo?.EngineId) ? string.Empty : $" ({engineInfo.EngineId})")}");
        Console.WriteLine($"Endpoint:         {endpoint}");
        if (!string.IsNullOrWhiteSpace(engineInfo?.Version))
        {
            Console.WriteLine($"Engine version:   {engineInfo.Version}");
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
