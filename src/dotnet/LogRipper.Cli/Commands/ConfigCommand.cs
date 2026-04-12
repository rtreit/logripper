using Grpc.Net.Client;
using LogRipper.Cli;
using LogRipper.Services;

namespace LogRipper.Cli.Commands;

internal static class ConfigCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string[] args, bool jsonOutput = false)
    {
        var client = new DeveloperControlService.DeveloperControlServiceClient(channel);

        for (var i = 0; i < args.Length; i++)
        {
            if (args[i] == "--reset")
            {
                var resetResponse = await client.ResetRuntimeConfigAsync(new ResetRuntimeConfigRequest());
                if (jsonOutput)
                {
                    JsonOutput.Print(resetResponse.Snapshot);
                }
                else
                {
                    Console.WriteLine("Runtime config reset to defaults.");
                    PrintSnapshot(resetResponse.Snapshot);
                }
                return 0;
            }

            if (args[i] == "--set" && i < args.Length - 1)
            {
                var kvp = args[++i];
                var eqIndex = kvp.IndexOf('=', StringComparison.Ordinal);
                if (eqIndex < 1)
                {
                    Console.Error.WriteLine("Expected KEY=VALUE format for --set.");
                    return 1;
                }

                var key = kvp[..eqIndex];
                var value = kvp[(eqIndex + 1)..];

                var applyRequest = new ApplyRuntimeConfigRequest();
                applyRequest.Mutations.Add(new RuntimeConfigMutation
                {
                    Key = key,
                    Value = value,
                    Kind = RuntimeConfigMutationKind.Set,
                });

                var applyResponse = await client.ApplyRuntimeConfigAsync(applyRequest);
                if (jsonOutput)
                {
                    JsonOutput.Print(applyResponse.Snapshot);
                }
                else
                {
                    Console.WriteLine("Config updated.");
                    PrintSnapshot(applyResponse.Snapshot);
                }
                return 0;
            }
        }

        var response = await client.GetRuntimeConfigAsync(new GetRuntimeConfigRequest());
        if (jsonOutput)
        {
            JsonOutput.Print(response.Snapshot);
        }
        else
        {
            PrintSnapshot(response.Snapshot);
        }
        return 0;
    }

    private static void PrintSnapshot(RuntimeConfigSnapshot? snapshot)
    {
        if (snapshot is null)
        {
            return;
        }

        foreach (var value in snapshot.Values)
        {
            var display = value.Redacted ? "<redacted>" : value.DisplayValue;
            var source = value.Overridden ? " (override)" : "";
            Console.WriteLine($"  {value.Key,-40} = {display}{source}");
        }
    }
}
