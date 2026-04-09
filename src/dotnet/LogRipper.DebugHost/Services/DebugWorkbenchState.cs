using System.Net.Sockets;
using LogRipper.DebugHost.Models;
using Microsoft.Extensions.Options;

namespace LogRipper.DebugHost.Services;

public sealed class DebugWorkbenchState
{
    private readonly DebugWorkbenchOptions _options;

    public DebugWorkbenchState(IOptions<DebugWorkbenchOptions> options)
    {
        _options = options.Value;
        EngineEndpoint = _options.DefaultEngineEndpoint;
    }

    public string EngineEndpoint { get; private set; }

    public TransportProbeResult? LastProbe { get; private set; }

    public void UpdateEngineEndpoint(string endpoint)
    {
        EngineEndpoint = endpoint.Trim();
    }

    public async Task<TransportProbeResult> ProbeAsync(CancellationToken cancellationToken = default)
    {
        var attemptedAt = DateTimeOffset.UtcNow;
        if (!Uri.TryCreate(EngineEndpoint, UriKind.Absolute, out var endpointUri))
        {
            LastProbe = new TransportProbeResult(false, "Endpoint is not a valid absolute URI.", attemptedAt, EngineEndpoint);
            return LastProbe;
        }

        var port = endpointUri.IsDefaultPort
            ? endpointUri.Scheme.Equals("https", StringComparison.OrdinalIgnoreCase) ? 443 : 80
            : endpointUri.Port;

        try
        {
            using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
            timeoutSource.CancelAfter(TimeSpan.FromSeconds(_options.ProbeTimeoutSeconds));

            using var client = new TcpClient();
            await client.ConnectAsync(endpointUri.Host, port, timeoutSource.Token);

            LastProbe = new TransportProbeResult(
                true,
                "TCP transport is reachable. gRPC method availability still depends on the Rust engine host implementing the service.",
                attemptedAt,
                EngineEndpoint);
            return LastProbe;
        }
        catch (Exception ex)
        {
            LastProbe = new TransportProbeResult(false, ex.Message, attemptedAt, EngineEndpoint);
            return LastProbe;
        }
    }
}
