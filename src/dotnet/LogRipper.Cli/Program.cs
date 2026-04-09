using Grpc.Core;
using Grpc.Net.Client;
using LogRipper.Cli;
using LogRipper.Cli.Commands;

var arguments = CliArgumentParser.Parse(args);

if (arguments.ShowHelp)
{
    return ShowHelp(arguments.Error);
}

using var channel = GrpcChannel.ForAddress(arguments.Endpoint);

try
{
    return arguments.Command switch
    {
        "status" => await StatusCommand.RunAsync(channel),
        _ => ShowHelp($"Unknown command: {arguments.Command}")
    };
}
catch (RpcException ex) when (ex.StatusCode == StatusCode.Unavailable)
{
    Console.Error.WriteLine($"Could not connect to LogRipper engine at {arguments.Endpoint}");
    Console.Error.WriteLine("Make sure the engine is running.");
    return 1;
}
catch (RpcException ex)
{
    Console.Error.WriteLine($"gRPC error: {ex.Status.Detail} ({ex.StatusCode})");
    return 1;
}

static int ShowHelp(string? error = null)
{
    if (error is not null)
    {
        Console.Error.WriteLine(error);
    }

    Console.WriteLine("""
        LogRipper CLI - validate and interact with the LogRipper engine

        Usage: logripper-cli [options] <command>

        Commands:
          status    Show engine sync status and QSO counts

        Options:
          --endpoint, -e <url>  Engine gRPC endpoint (default: http://localhost:50051)
                                Also configurable via LOGRIPPER_ENDPOINT env var
          --help, -h            Show this help
        """);

    return error is null ? 0 : 1;
}
