using Grpc.Core;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

internal sealed class StationProfileWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;
    private readonly DebugWorkbenchState _workbenchState;

    public StationProfileWorkbenchService(
        GrpcClientFactory clientFactory,
        DebugWorkbenchState workbenchState)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);
        ArgumentNullException.ThrowIfNull(workbenchState);

        _clientFactory = clientFactory;
        _workbenchState = workbenchState;
    }

    public Task RefreshAsync(CancellationToken cancellationToken = default)
    {
        return ExecuteAsync(
            client => client.ListStationProfilesAsync(
                    new ListStationProfilesRequest(),
                    cancellationToken: cancellationToken)
                .ResponseAsync,
            async client =>
            {
                var response = await client.GetActiveStationContextAsync(
                        new GetActiveStationContextRequest(),
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
                return response.Context ?? throw new InvalidOperationException("GetActiveStationContext returned no context payload.");
            });
    }

    public async Task<StationProfileRecord?> GetAsync(
        string profileId,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        try
        {
            var client = _clientFactory.CreateStationProfileClient();
            var response = await client.GetStationProfileAsync(
                    new GetStationProfileRequest { ProfileId = profileId },
                    cancellationToken: cancellationToken)
                .ResponseAsync;
            _workbenchState.ClearStationProfileError();
            return response.Profile;
        }
        catch (RpcException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Status.Detail);
            return null;
        }
        catch (OperationCanceledException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
            return null;
        }
        catch (InvalidOperationException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
            return null;
        }
    }

    public async Task<StationProfileRecord?> SaveAsync(
        SaveStationProfileRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        try
        {
            var client = _clientFactory.CreateStationProfileClient();
            var response = await client.SaveStationProfileAsync(
                    request,
                    cancellationToken: cancellationToken)
                .ResponseAsync;
            await RefreshStateAsync(client, cancellationToken);
            return response.Profile;
        }
        catch (RpcException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Status.Detail);
            return null;
        }
        catch (OperationCanceledException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
            return null;
        }
        catch (InvalidOperationException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
            return null;
        }
    }

    public Task<bool> SetActiveAsync(string profileId, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        return ExecuteMutationAsync(
            async client =>
            {
                await client.SetActiveStationProfileAsync(
                        new SetActiveStationProfileRequest { ProfileId = profileId },
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
            },
            cancellationToken);
    }

    public async Task<bool> UseSessionOverrideAsync(string profileId, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        var record = await GetAsync(profileId, cancellationToken);
        if (record?.Profile is null)
        {
            return false;
        }

        return await SetSessionOverrideAsync(record.Profile, cancellationToken);
    }

    public Task<bool> SetSessionOverrideAsync(
        StationProfile profile,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(profile);

        return ExecuteMutationAsync(
            async client =>
            {
                await client.SetSessionStationProfileOverrideAsync(
                        new SetSessionStationProfileOverrideRequest { Profile = profile },
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
            },
            cancellationToken);
    }

    public Task<bool> ClearSessionOverrideAsync(CancellationToken cancellationToken = default)
    {
        return ExecuteMutationAsync(
            async client =>
            {
                await client.ClearSessionStationProfileOverrideAsync(
                        new ClearSessionStationProfileOverrideRequest(),
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
            },
            cancellationToken);
    }

    public Task<bool> DeleteAsync(string profileId, CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        return ExecuteMutationAsync(
            async client =>
            {
                await client.DeleteStationProfileAsync(
                        new DeleteStationProfileRequest { ProfileId = profileId },
                        cancellationToken: cancellationToken)
                    .ResponseAsync;
            },
            cancellationToken);
    }

    private async Task ExecuteAsync(
        Func<StationProfileService.StationProfileServiceClient, Task<ListStationProfilesResponse>> catalogAction,
        Func<StationProfileService.StationProfileServiceClient, Task<ActiveStationContext>> contextAction)
    {
        try
        {
            var client = _clientFactory.CreateStationProfileClient();
            var catalog = await catalogAction(client);
            var context = await contextAction(client);
            _workbenchState.UpdateStationProfiles(catalog, context);
        }
        catch (RpcException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Status.Detail);
        }
        catch (OperationCanceledException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
        }
        catch (InvalidOperationException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
        }
    }

    private async Task<bool> ExecuteMutationAsync(
        Func<StationProfileService.StationProfileServiceClient, Task> mutationAction,
        CancellationToken cancellationToken)
    {
        try
        {
            var client = _clientFactory.CreateStationProfileClient();
            await mutationAction(client);
            await RefreshStateAsync(client, cancellationToken);
            return true;
        }
        catch (RpcException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Status.Detail);
            return false;
        }
        catch (OperationCanceledException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
            return false;
        }
        catch (InvalidOperationException ex)
        {
            _workbenchState.UpdateStationProfileError(ex.Message);
            return false;
        }
    }

    private async Task RefreshStateAsync(
        StationProfileService.StationProfileServiceClient client,
        CancellationToken cancellationToken)
    {
        var catalog = await client.ListStationProfilesAsync(
                new ListStationProfilesRequest(),
                cancellationToken: cancellationToken)
            .ResponseAsync;
        var context = await client.GetActiveStationContextAsync(
                new GetActiveStationContextRequest(),
                cancellationToken: cancellationToken)
            .ResponseAsync;
        var activeContext = context.Context
            ?? throw new InvalidOperationException("GetActiveStationContext returned no context payload.");
        _workbenchState.UpdateStationProfiles(catalog, activeContext);
    }
}
