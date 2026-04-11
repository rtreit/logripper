using Grpc.Core;
using Grpc.Net.Client;
using LogRipper.Cli;
using LogRipper.Cli.Commands;

var arguments = CliArgumentParser.Parse(args);

if (arguments.ShowHelp)
{
    return ShowHelp(arguments.Error);
}

if (!CliEndpointValidator.TryCreateEndpointUri(arguments.Endpoint, out var endpointUri))
{
    return ShowHelp($"The endpoint '{arguments.Endpoint}' must be a valid absolute http:// or https:// URI.");
}

var needsCallsign = arguments.Command is "lookup" or "stream-lookup" or "cache-check";
if (needsCallsign && string.IsNullOrEmpty(arguments.Callsign))
{
    Console.Error.WriteLine($"The '{arguments.Command}' command requires a callsign argument.");
    return 1;
}

try
{
    using var channel = GrpcChannel.ForAddress(endpointUri!);

    return arguments.Command switch
    {
        "status" => await StatusCommand.RunAsync(channel),
        "lookup" => await LookupCommand.RunAsync(channel, arguments.Callsign!, arguments.SkipCache),
        "stream-lookup" => await StreamLookupCommand.RunAsync(channel, arguments.Callsign!, arguments.SkipCache),
        "cache-check" => await CacheCheckCommand.RunAsync(channel, arguments.Callsign!),
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

        Usage: logripper-cli [options] <command> [arguments]

        Commands:
          status                         Show engine sync status and QSO counts
          lookup <callsign>              Look up a callsign via QRZ
          stream-lookup <callsign>       Streaming lookup with progressive updates
          cache-check <callsign>         Check if a callsign is in the cache

        Options:
          --endpoint, -e <url>  Engine gRPC endpoint (default: http://localhost:50051)
                                Also configurable via LOGRIPPER_ENDPOINT env var
          --skip-cache          Bypass the cache and force a fresh lookup
          --help, -h            Show this help
        """);

    return error is null ? 0 : 1;
}
