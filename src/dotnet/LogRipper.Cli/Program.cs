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

if (IsCommandHelp(arguments))
{
    return ShowCommandHelp(arguments.Command);
}

if (needsCallsign && string.IsNullOrEmpty(arguments.Callsign))
{
    return ShowCommandHelp(arguments.Command);
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

static bool IsCommandHelp(CliArguments arguments)
{
    return arguments.Callsign is "HELP" or "-?" or "--HELP"
        || arguments.RemainingArgs.Any(a => a is "help" or "-?" or "--help");
}

static int ShowCommandHelp(string command)
{
    var help = command switch
    {
        "log" => """
            Usage: log <callsign> <band> <mode> [options]

            Log a new QSO to the engine.

              --station <call>     Your station callsign (if not set via setup)
              --rst-sent <rst>     RST sent (e.g., 59, 599)
              --rst-rcvd <rst>     RST received
              --freq <khz>         Frequency in kHz (e.g., 14074)

            Examples:
              log W1AW 20m FT8
              log W1AW 40m CW --station AE7XI --rst-sent 599 --freq 7030
            """,
        "get" => """
            Usage: get <local-id>

            Retrieve a QSO by its local ID (returned by the log command).
            """,
        "list" => """
            Usage: list [options]

            List QSOs with optional filters.

              --callsign <call>    Filter by worked callsign
              --band <band>        Filter by band (e.g., 20m)
              --mode <mode>        Filter by mode (e.g., FT8)
              --limit <n>          Max results (default: 20)
            """,
        "delete" => """
            Usage: delete <local-id>

            Delete a QSO by its local ID.
            """,
        "import" => """
            Usage: import <file-path>

            Import QSOs from an ADIF (.adi) file.
            """,
        "export" => """
            Usage: export [options]

            Export QSOs to ADIF format.

              --file <path>        Write to file (default: stdout)
              --include-header     Include ADIF header
            """,
        "lookup" => """
            Usage: lookup <callsign> [--skip-cache]

            Look up a callsign via QRZ.
            """,
        "stream-lookup" => """
            Usage: stream-lookup <callsign> [--skip-cache]

            Streaming lookup with progressive state updates.
            """,
        "cache-check" => """
            Usage: cache-check <callsign>

            Check if a callsign is in the engine's cache.
            """,
        "config" => """
            Usage: config [options]

            View or modify runtime configuration.

              --set KEY=VALUE      Set a config value
              --reset              Reset all overrides to defaults
            """,
        "setup" => """
            Usage: setup

            Check first-run setup status.
            """,
        "status" => """
            Usage: status

            Show engine sync status and QSO counts.
            """,
        _ => $"No help available for '{command}'.",
    };

    Console.WriteLine(help);
    return 0;
}
