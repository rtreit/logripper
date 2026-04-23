using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class PurgeCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, string[] remainingArgs)
    {
        if (!TryParseArgs(remainingArgs, out var request, out var error))
        {
            Console.Error.WriteLine(error);
            return 1;
        }

        if (!request.Confirm)
        {
            Console.Write("This will permanently delete soft-deleted QSOs. Type YES to confirm: ");
            var input = Console.ReadLine();

            if (input != "YES")
            {
                Console.WriteLine("Purge canceled.");
                return 1;
            }

            request.Confirm = true;
        }

        var client = new LogbookService.LogbookServiceClient(channel);
        var response = await client.PurgeDeletedQsosAsync(request);

        Console.WriteLine($"Purged {response.PurgedCount} QSOs.");

        if (response.RemoteDeletesPushed > 0 || response.RemoteDeletesFailed > 0)
        {
            Console.WriteLine($"Remote deletes: {response.RemoteDeletesPushed} pushed, {response.RemoteDeletesFailed} failed.");
        }

        if (!string.IsNullOrEmpty(response.ErrorSummary))
        {
            Console.Error.WriteLine(response.ErrorSummary);
        }

        return 0;
    }

    private static bool TryParseArgs(string[] args, out PurgeDeletedQsosRequest request, out string? error)
    {
        request = new PurgeDeletedQsosRequest();
        error = null;

        for (var i = 0; i < args.Length; i++)
        {
            switch (args[i])
            {
                case "--older-than":
                    if (i + 1 >= args.Length)
                    {
                        error = "--older-than requires a duration value (e.g., 7.days)";
                        return false;
                    }

                    var timestamp = TimeParser.Parse(args[++i]);

                    if (timestamp is null)
                    {
                        error = $"Invalid duration for --older-than: '{args[i]}'. Use formats like 7.days, 30.days, 1.hours.";
                        return false;
                    }

                    request.OlderThan = timestamp;
                    break;
                case "--ids":
                    if (i + 1 >= args.Length)
                    {
                        error = "--ids requires a comma-separated list of local IDs";
                        return false;
                    }

                    var ids = args[++i].Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries);

                    if (ids.Length == 0)
                    {
                        error = "--ids requires at least one ID";
                        return false;
                    }

                    request.LocalIds.AddRange(ids);
                    break;
                case "--include-pending-remote-deletes":
                    request.IncludePendingRemoteDeletes = true;
                    break;
                case "--confirm":
                    request.Confirm = true;
                    break;
                default:
                    error = $"Unknown option: {args[i]}";
                    return false;
            }
        }

        return true;
    }
}
