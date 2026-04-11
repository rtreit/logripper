using Grpc.Core;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

internal sealed class SetupWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;
    private readonly DebugWorkbenchState _workbenchState;

    public SetupWorkbenchService(
        GrpcClientFactory clientFactory,
        DebugWorkbenchState workbenchState)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);
        ArgumentNullException.ThrowIfNull(workbenchState);

        _clientFactory = clientFactory;
        _workbenchState = workbenchState;
    }

    public Task<SetupStatusResponse?> RefreshAsync(CancellationToken cancellationToken = default)
    {
        return ExecuteAsync(
            client => client.GetSetupStatusAsync(
                    new GetSetupStatusRequest(),
                    cancellationToken: cancellationToken)
                .ResponseAsync);
    }

    public Task<SetupStatusResponse?> SaveAsync(
        SaveSetupRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        return ExecuteAsync(
            async client =>
            {
                var response = await client.SaveSetupAsync(
                        request,
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
                return response.Status;
            });
    }

    private async Task<SetupStatusResponse?> ExecuteAsync(
        Func<SetupService.SetupServiceClient, Task<SetupStatusResponse>> action)
    {
        try
        {
            using var channel = _clientFactory.CreateChannel();
            var client = new SetupService.SetupServiceClient(channel);
            var status = await action(client);
            _workbenchState.UpdateSetupStatus(status);
            return status;
        }
        catch (RpcException ex)
        {
            _workbenchState.UpdateSetupError(ex.Status.Detail);
            return null;
        }
        catch (OperationCanceledException ex)
        {
            _workbenchState.UpdateSetupError(ex.Message);
            return null;
        }
        catch (InvalidOperationException ex)
        {
            _workbenchState.UpdateSetupError(ex.Message);
            return null;
        }
    }
}
