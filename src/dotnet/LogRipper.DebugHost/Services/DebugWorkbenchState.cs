using System.Net.Sockets;
using Grpc.Core;
using Grpc.Net.Client;
using LogRipper.DebugHost.Models;
using LogRipper.Services;
using Microsoft.Extensions.Options;

namespace LogRipper.DebugHost.Services;

internal sealed class DebugWorkbenchState
{
    private readonly DebugWorkbenchOptions _options;

    public DebugWorkbenchState(IOptions<DebugWorkbenchOptions> options)
    {
        ArgumentNullException.ThrowIfNull(options);

        _options = options.Value;
        EngineEndpoint = _options.DefaultEngineEndpoint;
    }

    public string EngineEndpoint { get; private set; }

    public TransportProbeResult? LastProbe { get; private set; }

    public void UpdateEngineEndpoint(string endpoint)
    {
        ArgumentNullException.ThrowIfNull(endpoint);

        EngineEndpoint = endpoint.Trim();
    }

    public async Task<TransportProbeResult> ProbeAsync(CancellationToken cancellationToken = default)
    {
        var attemptedAt = DateTimeOffset.UtcNow;
        if (!Uri.TryCreate(EngineEndpoint, UriKind.Absolute, out var endpointUri))
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.InvalidEndpoint,
                "Endpoint is not a valid absolute URI.",
                "Correct the endpoint format to an absolute http:// or https:// URI.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }

        var port = endpointUri.IsDefaultPort
            ? endpointUri.Scheme.Equals("https", StringComparison.OrdinalIgnoreCase) ? 443 : 80
            : endpointUri.Port;

        try
        {
            using var tcpTimeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            tcpTimeoutSource.CancelAfter(TimeSpan.FromSeconds(_options.ProbeTimeoutSeconds));

            using var tcpClient = new TcpClient();
            await tcpClient.ConnectAsync(endpointUri.Host, port, tcpTimeoutSource.Token);
        }
        catch (OperationCanceledException ex)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.TcpUnreachable,
                $"TCP connection timed out: {ex.Message}",
                "Check that the engine host is running and reachable on the network.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (SocketException ex)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.TcpUnreachable,
                $"TCP connection failed: {ex.Message}",
                "Check that the engine host is running and reachable on the network.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (IOException ex)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.TcpUnreachable,
                $"TCP connection failed: {ex.Message}",
                "Check that the engine host is running and reachable on the network.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }

        // TCP succeeded; now probe gRPC capability via GetSyncStatus.
        var grpcChannel = GrpcChannel.ForAddress(endpointUri);
        try
        {
            using var grpcTimeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            grpcTimeoutSource.CancelAfter(TimeSpan.FromSeconds(_options.ProbeTimeoutSeconds));

            var client = new LogbookService.LogbookServiceClient(grpcChannel);
            var callOptions = new CallOptions(cancellationToken: grpcTimeoutSource.Token);
            await client.GetSyncStatusAsync(new SyncStatusRequest(), callOptions);

            LastProbe = new TransportProbeResult(
                true,
                EngineProbeStage.MethodSucceeded,
                "Engine is reachable and GetSyncStatus succeeded. Baseline service is live.",
                "The engine is operational. Proceed with logbook and lookup workflows.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (RpcException ex) when (ex.StatusCode == StatusCode.Unimplemented)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.MethodUnimplemented,
                "TCP and gRPC transport are reachable, but GetSyncStatus is not implemented.",
                "Implement GetSyncStatus in the Rust engine host to enable baseline service health checks.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (RpcException ex) when (ex.StatusCode == StatusCode.Unavailable)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.GrpcUnavailable,
                $"TCP is reachable but gRPC is unavailable: {ex.Status.Detail}",
                "Start the Rust gRPC engine host. The port is open but no gRPC service is responding.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (RpcException ex)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.GrpcUnavailable,
                $"gRPC call failed ({ex.StatusCode}): {ex.Status.Detail}",
                "Investigate the gRPC service error. Check engine logs for details.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (OperationCanceledException)
        {
            LastProbe = new TransportProbeResult(
                false,
                EngineProbeStage.GrpcUnavailable,
                "gRPC call timed out. TCP is reachable but the gRPC service did not respond in time.",
                "Start the Rust gRPC engine host. The port is open but no gRPC service is responding.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        finally
        {
            await grpcChannel.ShutdownAsync();
            grpcChannel.Dispose();
        }
    }
}
