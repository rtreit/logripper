using System.Diagnostics.CodeAnalysis;
using System.Net.Http;
using BenchmarkDotNet.Attributes;
using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.DebugHost.Benchmarks;

[SuppressMessage(
    "Design",
    "CA1515:Consider making public types internal",
    Justification = "BenchmarkDotNet benchmark types are discovered and activated via reflection.")]
public abstract class EngineBenchmarkBase
{
    private const int DefaultPort = 50051;

    [Params("localhost", "127.0.0.1")]
    public string Host { get; set; } = string.Empty;

    protected Uri EndpointUri => new($"http://{Host}:{ResolvePort()}", UriKind.Absolute);

    public async Task ValidateEndpointAsync()
    {
        using var channel = CreateChannel();
        var client = new LogbookService.LogbookServiceClient(channel);
        _ = await client.GetSyncStatusAsync(new GetSyncStatusRequest()).ResponseAsync;
    }

    protected GrpcChannel CreateChannel()
    {
        return GrpcChannel.ForAddress(EndpointUri);
    }

    protected GrpcChannel CreateTunedChannel()
    {
        return GrpcChannel.ForAddress(EndpointUri, new GrpcChannelOptions
        {
            HttpHandler = new SocketsHttpHandler
            {
                EnableMultipleHttp2Connections = true,
                PooledConnectionIdleTimeout = TimeSpan.FromMinutes(5),
                KeepAlivePingDelay = TimeSpan.FromSeconds(60),
                KeepAlivePingTimeout = TimeSpan.FromSeconds(30),
            }
        });
    }

    private static int ResolvePort()
    {
        var rawPort = Environment.GetEnvironmentVariable("LOGRIPPER_BENCHMARK_ENGINE_PORT");
        if (string.IsNullOrWhiteSpace(rawPort))
        {
            return DefaultPort;
        }

        if (int.TryParse(rawPort, out var port) && port is > 0 and <= 65535)
        {
            return port;
        }

        throw new InvalidOperationException(
            $"LOGRIPPER_BENCHMARK_ENGINE_PORT must be a valid TCP port. Current value: '{rawPort}'.");
    }
}
