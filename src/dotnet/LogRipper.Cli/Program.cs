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

var needsCallsign = arguments.Command is "lookup" or "stream-lookup" or "cache-check" or "log" or "get" or "delete";
if (needsCallsign && string.IsNullOrEmpty(arguments.Callsign))
{
    Console.Error.WriteLine($"The '{arguments.Command}' command requires an argument.");
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
        "log" => await LogQsoCommand.RunAsync(channel, arguments.Callsign!, arguments.RemainingArgs),
        "get" => await GetQsoCommand.RunAsync(channel, arguments.Callsign!),
        "list" => await ListQsosCommand.RunAsync(channel, arguments.RemainingArgs),
        "delete" => await DeleteQsoCommand.RunAsync(channel, arguments.Callsign!),
        "import" => await ImportAdifCommand.RunAsync(channel, arguments.Callsign ?? arguments.RemainingArgs.FirstOrDefault() ?? ""),
        "export" => await ExportAdifCommand.RunAsync(channel, arguments.RemainingArgs),
        "config" => await ConfigCommand.RunAsync(channel, arguments.RemainingArgs),
        "setup" => await SetupCommand.RunAsync(channel),
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
        LogRipper CLI

        Usage: logripper-cli [options] <command> [arguments]

        Logbook:
          log <call> <band> <mode>         Log a QSO (e.g., log W1AW 20m FT8)
          get <local-id>                   Get a QSO by ID
          list [filters]                   List QSOs (--callsign, --band, --mode, --limit)
          delete <local-id>                Delete a QSO

        ADIF:
          import <file>                    Import QSOs from an ADIF file
          export [--file out.adi]           Export QSOs to ADIF (stdout or file)

        Lookup:
          lookup <callsign>                Look up a callsign via QRZ
          stream-lookup <callsign>         Streaming lookup with progressive updates
          cache-check <callsign>           Check if a callsign is cached

        Engine:
          status                           Show sync status and QSO counts
          config [--set KEY=VALUE]         View or modify runtime config
          setup                            Check first-run setup status

        Options:
          --endpoint, -e <url>             Engine endpoint (default: http://127.0.0.1:50051)
          --skip-cache                     Bypass cache for lookup commands
          --help, -h                       Show this help
        """);

    return error is null ? 0 : 1;
}
