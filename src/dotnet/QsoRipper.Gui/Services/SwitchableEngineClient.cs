using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Gui.Services;

internal sealed class SwitchableEngineClient : IEngineClient, IDisposable
{
    private readonly object _gate = new();
    private readonly Func<string, IEngineClient> _clientFactory;
    private readonly List<IDisposable> _retiredClients = [];
    private IEngineClient _currentClient;

    public SwitchableEngineClient(
        EngineTargetProfile profile,
        string endpoint,
        Func<string, IEngineClient>? clientFactory = null)
    {
        ArgumentNullException.ThrowIfNull(profile);
        ArgumentException.ThrowIfNullOrWhiteSpace(endpoint);

        _clientFactory = clientFactory ?? CreateGrpcClient;
        CurrentProfile = profile;
        CurrentEndpoint = endpoint.Trim();
        _currentClient = _clientFactory(CurrentEndpoint);
    }

    public EngineTargetProfile CurrentProfile { get; private set; }

    public string CurrentEndpoint { get; private set; }

    public async Task<EngineSwitchResult> SwitchAsync(
        EngineTargetProfile profile,
        string endpoint,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(profile);
        ArgumentException.ThrowIfNullOrWhiteSpace(endpoint);

        var normalizedEndpoint = endpoint.Trim();
        if (string.Equals(CurrentEndpoint, normalizedEndpoint, StringComparison.OrdinalIgnoreCase)
            && string.Equals(CurrentProfile.ProfileId, profile.ProfileId, StringComparison.OrdinalIgnoreCase))
        {
            return new EngineSwitchResult(
                true,
                $"Already using {profile.DisplayName}.",
                CurrentProfile,
                CurrentEndpoint);
        }

        IEngineClient candidate;
        try
        {
            candidate = _clientFactory(normalizedEndpoint);
        }
        catch (ArgumentException ex)
        {
            return new EngineSwitchResult(
                false,
                $"Unable to initialize {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }
        catch (InvalidOperationException ex)
        {
            return new EngineSwitchResult(
                false,
                $"Unable to initialize {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }
        catch (UriFormatException ex)
        {
            return new EngineSwitchResult(
                false,
                $"Unable to initialize {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }

        try
        {
            await candidate.GetSyncStatusAsync(ct);
        }
        catch (RpcException ex)
        {
            DisposeClient(candidate);
            var detail = string.IsNullOrWhiteSpace(ex.Status.Detail)
                ? ex.StatusCode.ToString()
                : ex.Status.Detail;
            return new EngineSwitchResult(
                false,
                $"Unable to reach {profile.DisplayName}: {detail}",
                CurrentProfile,
                CurrentEndpoint);
        }
        catch (OperationCanceledException ex)
        {
            DisposeClient(candidate);
            return new EngineSwitchResult(
                false,
                $"Unable to reach {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }
        catch (HttpRequestException ex)
        {
            DisposeClient(candidate);
            return new EngineSwitchResult(
                false,
                $"Unable to reach {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }
        catch (IOException ex)
        {
            DisposeClient(candidate);
            return new EngineSwitchResult(
                false,
                $"Unable to reach {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }
        catch (InvalidOperationException ex)
        {
            DisposeClient(candidate);
            return new EngineSwitchResult(
                false,
                $"Unable to reach {profile.DisplayName}: {ex.Message}",
                CurrentProfile,
                CurrentEndpoint);
        }

        IEngineClient previous;
        lock (_gate)
        {
            previous = _currentClient;
            _currentClient = candidate;
            CurrentProfile = profile;
            CurrentEndpoint = normalizedEndpoint;
            if (previous is IDisposable disposable)
            {
                _retiredClients.Add(disposable);
            }
        }

        return new EngineSwitchResult(
            true,
            $"Switched to {profile.DisplayName}.",
            CurrentProfile,
            CurrentEndpoint);
    }

    public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) =>
        SnapshotClient().GetWizardStateAsync(ct);

    public Task<ValidateSetupStepResponse> ValidateStepAsync(
        ValidateSetupStepRequest request,
        CancellationToken ct = default) =>
        SnapshotClient().ValidateStepAsync(request, ct);

    public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(
        string username,
        string password,
        CancellationToken ct = default) =>
        SnapshotClient().TestQrzCredentialsAsync(username, password, ct);

    public Task<SaveSetupResponse> SaveSetupAsync(
        SaveSetupRequest request,
        CancellationToken ct = default) =>
        SnapshotClient().SaveSetupAsync(request, ct);

    public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) =>
        SnapshotClient().GetSetupStatusAsync(ct);

    public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(
        string apiKey,
        CancellationToken ct = default) =>
        SnapshotClient().TestQrzLogbookCredentialsAsync(apiKey, ct);

    public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) =>
        SnapshotClient().ListRecentQsosAsync(limit, ct);

    public Task<UpdateQsoResponse> UpdateQsoAsync(
        QsoRecord qso,
        bool syncToQrz = false,
        CancellationToken ct = default) =>
        SnapshotClient().UpdateQsoAsync(qso, syncToQrz, ct);

    public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) =>
        SnapshotClient().SyncWithQrzAsync(ct);

    public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) =>
        SnapshotClient().GetSyncStatusAsync(ct);

    public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) =>
        SnapshotClient().LookupCallsignAsync(callsign, ct);

    public Task<DeleteQsoResponse> DeleteQsoAsync(
        string localId,
        bool deleteFromQrz = false,
        CancellationToken ct = default) =>
        SnapshotClient().DeleteQsoAsync(localId, deleteFromQrz, ct);

    public Task<LogQsoResponse> LogQsoAsync(
        QsoRecord qso,
        bool syncToQrz = false,
        CancellationToken ct = default) =>
        SnapshotClient().LogQsoAsync(qso, syncToQrz, ct);

    public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) =>
        SnapshotClient().GetRigSnapshotAsync(ct);

    public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
        SnapshotClient().GetRigStatusAsync(ct);

    public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) =>
        SnapshotClient().GetCurrentSpaceWeatherAsync(ct);

    public void Dispose()
    {
        var disposables = new List<IDisposable>();
        lock (_gate)
        {
            if (_currentClient is IDisposable currentDisposable)
            {
                disposables.Add(currentDisposable);
            }

            disposables.AddRange(_retiredClients);
            _retiredClients.Clear();
        }

        foreach (var disposable in disposables)
        {
            disposable.Dispose();
        }
    }

    private IEngineClient SnapshotClient()
    {
        lock (_gate)
        {
            return _currentClient;
        }
    }

    private static EngineGrpcService CreateGrpcClient(string endpoint)
    {
        return new EngineGrpcService(GrpcChannel.ForAddress(endpoint));
    }

    private static void DisposeClient(IEngineClient candidate)
    {
        if (candidate is IDisposable disposable)
        {
            disposable.Dispose();
        }
    }
}
