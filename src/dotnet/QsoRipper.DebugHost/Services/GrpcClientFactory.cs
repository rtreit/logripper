using System.Net.Http;
using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class GrpcClientFactory : IDisposable
{
    private readonly DebugWorkbenchState _state;
    private readonly Lock _lock = new();
    private volatile CachedChannel? _cached;
    private bool _disposed;

    private sealed record CachedChannel(GrpcChannel Channel, string Endpoint);

    public GrpcClientFactory(DebugWorkbenchState state)
    {
        ArgumentNullException.ThrowIfNull(state);

        _state = state;
    }

    public GrpcChannel GetOrCreateChannel()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var currentEndpoint = _state.EngineEndpoint;
        var cached = _cached;

        if (cached is not null && string.Equals(cached.Endpoint, currentEndpoint, StringComparison.Ordinal))
        {
            return cached.Channel;
        }

        lock (_lock)
        {
            cached = _cached;
            if (cached is not null && string.Equals(cached.Endpoint, currentEndpoint, StringComparison.Ordinal))
            {
                return cached.Channel;
            }

            cached?.Channel.Dispose();

            if (!Uri.TryCreate(currentEndpoint, UriKind.Absolute, out var endpointUri))
            {
                throw new InvalidOperationException(
                    "The engine endpoint must be a valid absolute URI before creating a gRPC channel.");
            }

            var channel = GrpcChannel.ForAddress(endpointUri, CreateChannelOptions());
            _cached = new CachedChannel(channel, currentEndpoint);
            return channel;
        }
    }

    public LookupService.LookupServiceClient CreateLookupClient()
    {
        return new LookupService.LookupServiceClient(GetOrCreateChannel());
    }

    public LogbookService.LogbookServiceClient CreateLogbookClient()
    {
        return new LogbookService.LogbookServiceClient(GetOrCreateChannel());
    }

    public DeveloperControlService.DeveloperControlServiceClient CreateDeveloperControlClient()
    {
        return new DeveloperControlService.DeveloperControlServiceClient(GetOrCreateChannel());
    }

    public SetupService.SetupServiceClient CreateSetupClient()
    {
        return new SetupService.SetupServiceClient(GetOrCreateChannel());
    }

    public StationProfileService.StationProfileServiceClient CreateStationProfileClient()
    {
        return new StationProfileService.StationProfileServiceClient(GetOrCreateChannel());
    }

    public RigControlService.RigControlServiceClient CreateRigControlClient()
    {
        return new RigControlService.RigControlServiceClient(GetOrCreateChannel());
    }

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _cached?.Channel.Dispose();
        _cached = null;
    }

    private static GrpcChannelOptions CreateChannelOptions()
    {
        return new GrpcChannelOptions
        {
            HttpHandler = new SocketsHttpHandler
            {
                EnableMultipleHttp2Connections = true,
                PooledConnectionIdleTimeout = TimeSpan.FromMinutes(5),
                KeepAlivePingDelay = TimeSpan.FromSeconds(60),
                KeepAlivePingTimeout = TimeSpan.FromSeconds(30),
            }
        };
    }
}
