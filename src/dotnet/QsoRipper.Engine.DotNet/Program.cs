using System.Net;
using System.Text.Json;
using Microsoft.AspNetCore.Server.Kestrel.Core;
using QsoRipper.Engine.DotNet;
using QsoRipper.Engine.Lookup;
using QsoRipper.Engine.Lookup.Qrz;
using QsoRipper.Engine.QrzLogbook;
using QsoRipper.Engine.RigControl;
using QsoRipper.Engine.SpaceWeather;
using QsoRipper.Engine.Storage;
using QsoRipper.Engine.Storage.Memory;
using QsoRipper.Engine.Storage.Sqlite;

var options = ManagedEngineHostOptions.Parse(args);

var builder = WebApplication.CreateBuilder(args);
builder.WebHost.ConfigureKestrel(kestrel => ConfigureListenEndpoint(kestrel, options.ListenAddress));
builder.Services.AddGrpc();

var storage = CreateStorage();
builder.Services.AddSingleton(storage);

var lookupCoordinator = CreateLookupCoordinator(storage, options.ConfigPath);
builder.Services.AddSingleton(lookupCoordinator);

var rigControlMonitor = CreateRigControlMonitor();
var spaceWeatherMonitor = CreateSpaceWeatherMonitor();
var syncEngine = CreateSyncEngine();

builder.Services.AddSingleton(provider => new ManagedEngineState(
    options.ConfigPath,
    provider.GetRequiredService<IEngineStorage>(),
    provider.GetRequiredService<ILookupCoordinator>(),
    rigControlMonitor,
    spaceWeatherMonitor,
    syncEngine));

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

Console.WriteLine($"Starting QsoRipper .NET engine on {options.ListenAddress} using config {options.ConfigPath} (storage: {storage.BackendName})");
await app.RunAsync();

static QrzSyncEngine? CreateSyncEngine()
{
    var apiKey = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_LOGBOOK_API_KEY")?.Trim();
    if (string.IsNullOrWhiteSpace(apiKey))
    {
        Console.WriteLine("QRZ Logbook sync: disabled (QSORIPPER_QRZ_LOGBOOK_API_KEY not set)");
        return null;
    }

    // HttpClient is intentionally not disposed — it is a singleton owned by the client for the app lifetime.
#pragma warning disable CA2000 // Dispose objects before losing scope
    var client = new QrzLogbookClient(new HttpClient { Timeout = TimeSpan.FromSeconds(30) }, apiKey);
#pragma warning restore CA2000
    Console.WriteLine("QRZ Logbook sync: enabled");
    return new QrzSyncEngine(client);
}

static IEngineStorage CreateStorage()
{
    var backend = Environment.GetEnvironmentVariable("QSORIPPER_STORAGE_BACKEND")?.Trim();
    if (string.Equals(backend, "sqlite", StringComparison.OrdinalIgnoreCase))
    {
        var path = Environment.GetEnvironmentVariable("QSORIPPER_STORAGE_PATH")?.Trim();
        var storageBuilder = new SqliteStorageBuilder();
        if (!string.IsNullOrWhiteSpace(path))
        {
            storageBuilder.Path(path);
        }

        return storageBuilder.Build();
    }

    return new MemoryStorage();
}

static ILookupCoordinator CreateLookupCoordinator(IEngineStorage storage, string? configPath = null)
{
    var username = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_XML_USERNAME")?.Trim();
    var password = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_XML_PASSWORD")?.Trim();

    if ((string.IsNullOrWhiteSpace(username) || string.IsNullOrWhiteSpace(password)) && configPath is not null)
    {
        var persisted = TryLoadPersistedConfig(configPath);
        if (persisted is not null)
        {
            if (string.IsNullOrWhiteSpace(username))
            {
                username = persisted.QrzXmlUsername?.Trim();
            }

            if (string.IsNullOrWhiteSpace(password))
            {
                password = persisted.QrzXmlPassword?.Trim();
            }
        }
    }

    ICallsignProvider provider;
    if (!string.IsNullOrWhiteSpace(username) && !string.IsNullOrWhiteSpace(password))
    {
        // HttpClient is intentionally not disposed — it is a singleton owned by the provider for the app lifetime.
#pragma warning disable CA2000 // Dispose objects before losing scope
        var httpClient = new HttpClient { Timeout = TimeSpan.FromSeconds(8) };
#pragma warning restore CA2000
        provider = new QrzXmlProvider(httpClient, username, password);
    }
    else
    {
        provider = new DisabledCallsignProvider();
    }

    return new LookupCoordinator(provider, storage.LookupSnapshots);
}

static RigControlMonitor? CreateRigControlMonitor()
{
    var enabled = Environment.GetEnvironmentVariable("QSORIPPER_RIGCTLD_ENABLED")?.Trim();

    // Disabled explicitly: "false" or "0".
    if (string.Equals(enabled, "false", StringComparison.OrdinalIgnoreCase) || enabled == "0")
    {
        return null;
    }

    // Not set at all: disabled by default in the .NET engine (mirrors Rust: enabled by default
    // only when the env var is absent, but .NET engine defaults to disabled for safety).
    if (string.IsNullOrWhiteSpace(enabled))
    {
        return null;
    }

    var host = Environment.GetEnvironmentVariable("QSORIPPER_RIGCTLD_HOST")?.Trim();
    if (string.IsNullOrWhiteSpace(host))
    {
        host = RigctldProvider.DefaultHost;
    }

    var portStr = Environment.GetEnvironmentVariable("QSORIPPER_RIGCTLD_PORT")?.Trim();
    var port = int.TryParse(portStr, out var parsedPort) ? parsedPort : RigctldProvider.DefaultPort;

    var readTimeoutStr = Environment.GetEnvironmentVariable("QSORIPPER_RIGCTLD_READ_TIMEOUT_MS")?.Trim();
    var readTimeoutMs = int.TryParse(readTimeoutStr, out var parsedTimeout) ? parsedTimeout : RigctldProvider.DefaultReadTimeoutMs;

    var staleThresholdStr = Environment.GetEnvironmentVariable("QSORIPPER_RIGCTLD_STALE_THRESHOLD_MS")?.Trim();
    var staleThresholdMs = int.TryParse(staleThresholdStr, out var parsedStale) ? parsedStale : RigControlMonitor.DefaultStaleThresholdMs;

    var provider = new RigctldProvider(host, port, TimeSpan.FromMilliseconds(readTimeoutMs));
    Console.WriteLine($"Rig control enabled: rigctld at {host}:{port} (timeout {readTimeoutMs}ms, stale {staleThresholdMs}ms)");
    return new RigControlMonitor(provider, TimeSpan.FromMilliseconds(staleThresholdMs));
}

static SpaceWeatherMonitor? CreateSpaceWeatherMonitor()
{
    var config = NoaaSpaceWeatherConfig.FromEnvironment();
    if (!config.Enabled)
    {
        Console.WriteLine("NOAA space weather: disabled");
        return null;
    }

    // HttpClient is intentionally not disposed — it is a singleton owned by the provider for the app lifetime.
#pragma warning disable CA2000 // Dispose objects before losing scope
    var httpClient = new HttpClient { Timeout = config.HttpTimeout };
#pragma warning restore CA2000
    var provider = new NoaaSpaceWeatherProvider(httpClient, config);
    Console.WriteLine($"NOAA space weather: enabled (refresh every {config.RefreshInterval.TotalSeconds}s, stale after {config.StaleAfter.TotalSeconds}s)");
    return new SpaceWeatherMonitor(provider, config.RefreshInterval, config.StaleAfter);
}

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

static ManagedEnginePersistedState? TryLoadPersistedConfig(string configPath)
{
    try
    {
        if (!File.Exists(configPath))
        {
            return null;
        }

        var json = File.ReadAllText(configPath);
        return JsonSerializer.Deserialize<ManagedEnginePersistedState>(json, new JsonSerializerOptions
        {
            PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        });
    }
#pragma warning disable CA1031 // Do not catch general exception types
    catch
#pragma warning restore CA1031
    {
        return null;
    }
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
