using Grpc.Core;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

internal sealed class RuntimeConfigWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;
    private readonly DebugWorkbenchState _workbenchState;

    public RuntimeConfigWorkbenchService(
        GrpcClientFactory clientFactory,
        DebugWorkbenchState workbenchState)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);
        ArgumentNullException.ThrowIfNull(workbenchState);

        _clientFactory = clientFactory;
        _workbenchState = workbenchState;
    }

    public Task<RuntimeConfigSnapshot?> RefreshAsync(CancellationToken cancellationToken = default)
    {
        return ExecuteAsync(
            async client =>
            {
                var response = await client.GetRuntimeConfigAsync(
                        new GetRuntimeConfigRequest(),
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
                return response.Snapshot ?? throw new InvalidOperationException("GetRuntimeConfig returned no snapshot payload.");
            },
            cancellationToken);
    }

    public Task<RuntimeConfigSnapshot?> ApplyAsync(
        IEnumerable<RuntimeConfigMutation> mutations,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(mutations);

        return ExecuteAsync(
            async client =>
            {
                var response = await client.ApplyRuntimeConfigAsync(
                    new ApplyRuntimeConfigRequest
                    {
                        Mutations = { mutations }
                    },
                    cancellationToken: cancellationToken)
                    .ResponseAsync;
                return response.Snapshot ?? throw new InvalidOperationException("ApplyRuntimeConfig returned no snapshot payload.");
            },
            cancellationToken);
    }

    public Task<RuntimeConfigSnapshot?> ResetAsync(
        IEnumerable<string>? keys = null,
        CancellationToken cancellationToken = default)
    {
        var request = new ResetRuntimeConfigRequest();
        if (keys is not null)
        {
            request.Keys.AddRange(keys);
        }

        return ExecuteAsync(
            async client =>
            {
                var response = await client.ResetRuntimeConfigAsync(
                        request,
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
                return response.Snapshot ?? throw new InvalidOperationException("ResetRuntimeConfig returned no snapshot payload.");
            },
            cancellationToken);
    }

    private async Task<RuntimeConfigSnapshot?> ExecuteAsync(
        Func<DeveloperControlService.DeveloperControlServiceClient, Task<RuntimeConfigSnapshot>> action,
        CancellationToken cancellationToken)
    {
        try
        {
            var client = _clientFactory.CreateDeveloperControlClient();
            var snapshot = await action(client);
            _workbenchState.UpdateRuntimeConfig(snapshot);
            return snapshot;
        }
        catch (RpcException ex)
        {
            _workbenchState.UpdateRuntimeConfigError(ex.Status.Detail);
            return null;
        }
        catch (OperationCanceledException ex)
        {
            _workbenchState.UpdateRuntimeConfigError(ex.Message);
            return null;
        }
        catch (InvalidOperationException ex)
        {
            _workbenchState.UpdateRuntimeConfigError(ex.Message);
            return null;
        }
    }
}
