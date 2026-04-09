using Grpc.Net.Client;
using LogRipper.Services;

const string DefaultEndpoint = "http://localhost:50051";

var endpoint = GetEndpoint(args);
var command = GetCommand(args);

using var channel = GrpcChannel.ForAddress(endpoint);

try
{
    var exitCode = command switch
    {
        "status" => await Commands.StatusCommand.RunAsync(channel),
        "help" or "--help" or "-h" => ShowHelp(),
        _ => ShowHelp($"Unknown command: {command}")
    };

    return exitCode;
}
catch (Grpc.Core.RpcException ex) when (ex.StatusCode == Grpc.Core.StatusCode.Unavailable)
{
    Console.Error.WriteLine($"Could not connect to LogRipper engine at {endpoint}");
    Console.Error.WriteLine("Make sure the engine is running.");
    return 1;
}
catch (Grpc.Core.RpcException ex)
{
    Console.Error.WriteLine($"gRPC error: {ex.Status.Detail} ({ex.StatusCode})");
    return 1;
}

string GetEndpoint(string[] args)
{
    for (var i = 0; i < args.Length - 1; i++)
    {
        if (args[i] is "--endpoint" or "-e")
            {
                return args[i + 1];
            }
    }

    return Environment.GetEnvironmentVariable("LOGRIPPER_ENDPOINT") ?? DefaultEndpoint;
}

string GetCommand(string[] args)
{
    foreach (var arg in args)
    {
        if (!arg.StartsWith('-'))
        {
            return arg;
        }
    }

    return "help";
}

int ShowHelp(string? error = null)
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
