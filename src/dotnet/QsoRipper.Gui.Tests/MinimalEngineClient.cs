using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

/// <summary>
/// No-op <see cref="IEngineClient"/> for unit tests that exercise
/// view-model logic without touching the engine. Methods throw on use
/// so accidental engine calls fail loudly; only the small subset
/// needed to construct view models returns benign defaults.
/// </summary>
internal sealed class MinimalEngineClient : IEngineClient
{
    public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<ValidateSetupStepResponse> ValidateStepAsync(ValidateSetupStepRequest request, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(string username, string password, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(string apiKey, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) => Task.FromResult<IReadOnlyList<QsoRecord>>([]);
    public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) => Task.FromResult(new LogQsoResponse { LocalId = "x" });
    public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<ComputeGreatCircleResponse> ComputeGreatCircleAsync(ComputeGreatCircleRequest request, CancellationToken ct = default) => throw new NotImplementedException();
    public Task<GetActiveStationContextResponse> GetActiveStationContextAsync(CancellationToken ct = default) => throw new NotImplementedException();
    public Task<PurgeDeletedQsosResponse> PurgeDeletedQsosAsync(IReadOnlyList<string>? localIds = null, Timestamp? olderThan = null, bool includePendingRemoteDeletes = false, CancellationToken ct = default) => throw new NotImplementedException();
}
