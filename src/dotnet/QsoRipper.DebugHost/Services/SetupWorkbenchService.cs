using Grpc.Core;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

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

    public Task<SetupStatus?> RefreshAsync(CancellationToken cancellationToken = default)
    {
        return ExecuteAsync(
            async client =>
            {
                var response = await client.GetSetupStatusAsync(
                        new GetSetupStatusRequest(),
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
                return response.Status ?? throw new InvalidOperationException("GetSetupStatus returned no status payload.");
            });
    }

    public Task<SetupStatus?> SaveAsync(
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
                return response.Status ?? throw new InvalidOperationException("SaveSetup returned no status payload.");
            });
    }

    private async Task<SetupStatus?> ExecuteAsync(
        Func<SetupService.SetupServiceClient, Task<SetupStatus>> action)
    {
        try
        {
            var client = _clientFactory.CreateSetupClient();
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
