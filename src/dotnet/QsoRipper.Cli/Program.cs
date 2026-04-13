using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Cli.Commands;

var arguments = CliArgumentParser.Parse(args);

if (arguments.ShowHelp)
{
    return ShowHelp(arguments.Error);
}

if (!CliEndpointValidator.TryCreateEndpointUri(arguments.Endpoint, out var endpointUri))
{
    return ShowHelp($"The endpoint '{arguments.Endpoint}' must be a valid absolute http:// or https:// URI.");
}

var needsCallsign = CliCommandMetadata.RequiresPrimaryArgument(arguments.Command);

if (CliCommandMetadata.IsCommandHelp(arguments))
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
        "status" => await StatusCommand.RunAsync(channel, arguments.JsonOutput),
        "lookup" => await LookupCommand.RunAsync(channel, arguments.Callsign!, arguments.SkipCache, arguments.JsonOutput),
        "stream-lookup" => await StreamLookupCommand.RunAsync(channel, arguments.Callsign!, arguments.SkipCache),
        "cache-check" => await CacheCheckCommand.RunAsync(channel, arguments.Callsign!, arguments.JsonOutput),
        "log" => await LogQsoCommand.RunAsync(channel, arguments.Callsign!, arguments.RemainingArgs),
        "get" => await GetQsoCommand.RunAsync(channel, arguments.Callsign!, arguments.JsonOutput),
        "list" => await ListQsosCommand.RunAsync(channel, arguments.RemainingArgs, arguments.JsonOutput),
        "update" => await UpdateQsoCommand.RunAsync(channel, arguments.Callsign!, arguments.RemainingArgs),
        "delete" => await DeleteQsoCommand.RunAsync(channel, arguments.Callsign!),
        "import" => await ImportAdifCommand.RunAsync(channel, arguments.Callsign ?? arguments.RemainingArgs.FirstOrDefault() ?? ""),
        "export" => await ExportAdifCommand.RunAsync(channel, arguments.RemainingArgs),
        "config" => await ConfigCommand.RunAsync(channel, arguments.RemainingArgs, arguments.JsonOutput),
        "setup" => await SetupCommand.RunAsync(channel, arguments),
        _ => ShowHelp($"Unknown command: {arguments.Command}")
    };
}
catch (RpcException ex) when (ex.StatusCode == StatusCode.Unavailable)
{
    Console.Error.WriteLine($"Could not connect to QsoRipper engine at {arguments.Endpoint}");
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

    Console.WriteLine(CliHelpText.GetGeneralHelp());

    return error is null ? 0 : 1;
}

static int ShowCommandHelp(string command)
{
    Console.WriteLine(CliHelpText.GetCommandHelp(command));
    return 0;
}
