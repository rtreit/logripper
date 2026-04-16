using System.Net;
using Microsoft.AspNetCore.Server.Kestrel.Core;
using QsoRipper.Engine.DotNet;
using QsoRipper.Engine.Storage;
using QsoRipper.Engine.Storage.Memory;

var options = ManagedEngineHostOptions.Parse(args);

var builder = WebApplication.CreateBuilder(args);
builder.WebHost.ConfigureKestrel(kestrel => ConfigureListenEndpoint(kestrel, options.ListenAddress));
builder.Services.AddGrpc();
builder.Services.AddSingleton<IEngineStorage>(new MemoryStorage());
builder.Services.AddSingleton(provider => new ManagedEngineState(options.ConfigPath, provider.GetRequiredService<IEngineStorage>()));

var app = builder.Build();
app.MapGrpcService<ManagedEngineInfoGrpcService>();
app.MapGrpcService<ManagedSetupGrpcService>();
app.MapGrpcService<ManagedStationProfileGrpcService>();
app.MapGrpcService<ManagedDeveloperControlGrpcService>();
app.MapGrpcService<ManagedLogbookGrpcService>();
app.MapGrpcService<ManagedLookupGrpcService>();
app.MapGrpcService<ManagedRigControlGrpcService>();
app.MapGrpcService<ManagedSpaceWeatherGrpcService>();
app.MapGet("/", () => "QsoRipper .NET engine host. Use a gRPC client.");

Console.WriteLine($"Starting QsoRipper .NET engine on {options.ListenAddress} using config {options.ConfigPath}");
await app.RunAsync();

static void ConfigureListenEndpoint(KestrelServerOptions options, string listenAddress)
{
    var parts = listenAddress.Split(':', 2, StringSplitOptions.TrimEntries);
    if (parts.Length != 2 || !int.TryParse(parts[1], out var port) || port is < 1 or > 65535)
    {
        throw new InvalidOperationException($"Invalid listen address '{listenAddress}'. Expected host:port.");
    }

    var host = parts[0];
    if (host.Equals("localhost", StringComparison.OrdinalIgnoreCase))
    {
        options.ListenLocalhost(port, configure => configure.Protocols = HttpProtocols.Http2);
        return;
    }

    if (IPAddress.TryParse(host, out var ipAddress))
    {
        options.Listen(ipAddress, port, configure => configure.Protocols = HttpProtocols.Http2);
        return;
    }

    options.ListenAnyIP(port, configure => configure.Protocols = HttpProtocols.Http2);
}

internal sealed record ManagedEngineHostOptions(string ListenAddress, string ConfigPath)
{
    public const string DefaultListenAddress = "127.0.0.1:50052";
    public const string ConfigPathEnvironmentVariable = "QSORIPPER_CONFIG_PATH";
    public const string ListenAddressEnvironmentVariable = "QSORIPPER_SERVER_ADDR";

    public static ManagedEngineHostOptions Parse(string[] args)
    {
        var listenAddress = Environment.GetEnvironmentVariable(ListenAddressEnvironmentVariable) ?? DefaultListenAddress;
        var configPath = Environment.GetEnvironmentVariable(ConfigPathEnvironmentVariable) ?? GetDefaultConfigPath();

        for (var index = 0; index < args.Length; index++)
        {
            switch (args[index])
            {
                case "--listen":
                    if (index == args.Length - 1)
                    {
                        throw new InvalidOperationException("Missing value for --listen.");
                    }

                    listenAddress = args[++index];
                    break;
                case "--config":
                    if (index == args.Length - 1)
                    {
                        throw new InvalidOperationException("Missing value for --config.");
                    }

                    configPath = args[++index];
                    break;
                case "--help":
                case "-h":
                    PrintHelp();
                    Environment.Exit(0);
                    break;
                default:
                    throw new InvalidOperationException($"Unknown argument: {args[index]}");
            }
        }

        return new ManagedEngineHostOptions(listenAddress, Path.GetFullPath(configPath));
    }

    private static string GetDefaultConfigPath()
    {
        var baseDirectory = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
        return Path.Combine(baseDirectory, "QsoRipper", "dotnet-engine.json");
    }

    private static void PrintHelp()
    {
        Console.WriteLine(
            """
            QsoRipper .NET engine host

            Usage:
              dotnet run --project src\dotnet\QsoRipper.Engine.DotNet -- [--listen 127.0.0.1:50052] [--config path\to\dotnet-engine.json]

            Environment:
              QSORIPPER_SERVER_ADDR   Overrides the bind address
              QSORIPPER_CONFIG_PATH   Overrides the managed-engine config path
            """);
    }
}
