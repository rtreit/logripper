using System.Net.Http;
using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

internal sealed class GrpcClientFactory : IDisposable
{
    private readonly DebugWorkbenchState _state;
    private readonly Lock _lock = new();
    private GrpcChannel? _channel;
    private string? _channelEndpoint;
    private bool _disposed;

    public GrpcClientFactory(DebugWorkbenchState state)
    {
        ArgumentNullException.ThrowIfNull(state);

        _state = state;
    }

    public GrpcChannel GetOrCreateChannel()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);

        var currentEndpoint = _state.EngineEndpoint;

        lock (_lock)
        {
            if (_channel is not null && string.Equals(_channelEndpoint, currentEndpoint, StringComparison.Ordinal))
            {
                return _channel;
            }

            _channel?.Dispose();

            if (!Uri.TryCreate(currentEndpoint, UriKind.Absolute, out var endpointUri))
            {
                throw new InvalidOperationException(
                    "The engine endpoint must be a valid absolute URI before creating a gRPC channel.");
            }

            _channel = GrpcChannel.ForAddress(endpointUri, CreateChannelOptions());
            _channelEndpoint = currentEndpoint;
            return _channel;
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

    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }

        _disposed = true;
        _channel?.Dispose();
        _channel = null;
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
