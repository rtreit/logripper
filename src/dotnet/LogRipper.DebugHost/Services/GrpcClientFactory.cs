using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

internal sealed class GrpcClientFactory
{
    private readonly DebugWorkbenchState _state;

    public GrpcClientFactory(DebugWorkbenchState state)
    {
        ArgumentNullException.ThrowIfNull(state);

        _state = state;
    }

    public GrpcChannel CreateChannel()
    {
        if (!Uri.TryCreate(_state.EngineEndpoint, UriKind.Absolute, out var endpointUri))
        {
            throw new InvalidOperationException("The engine endpoint must be a valid absolute URI before creating a gRPC channel.");
        }

        return GrpcChannel.ForAddress(endpointUri);
    }

    public LookupService.LookupServiceClient CreateLookupClient()
    {
        return new LookupService.LookupServiceClient(CreateChannel());
    }

    public LogbookService.LogbookServiceClient CreateLogbookClient()
    {
        return new LogbookService.LogbookServiceClient(CreateChannel());
    }

    public DeveloperControlService.DeveloperControlServiceClient CreateDeveloperControlClient()
    {
        return new DeveloperControlService.DeveloperControlServiceClient(CreateChannel());
    }

    public SetupService.SetupServiceClient CreateSetupClient()
    {
        return new SetupService.SetupServiceClient(CreateChannel());
    }

    public StationProfileService.StationProfileServiceClient CreateStationProfileClient()
    {
        return new StationProfileService.StationProfileServiceClient(CreateChannel());
    }
}
