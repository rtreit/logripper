using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Gui.Services;

internal interface IEngineClient
{
    Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default);

    Task<ValidateSetupStepResponse> ValidateStepAsync(
        ValidateSetupStepRequest request,
        CancellationToken ct = default);

    Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(
        string username,
        string password,
        CancellationToken ct = default);

    Task<SaveSetupResponse> SaveSetupAsync(
        SaveSetupRequest request,
        CancellationToken ct = default);

    Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default);

    Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(
        string apiKey,
        CancellationToken ct = default);

    Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default);

    Task<UpdateQsoResponse> UpdateQsoAsync(
        QsoRecord qso,
        bool syncToQrz = false,
        CancellationToken ct = default);

    Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default);

    Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default);

    Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default);

    Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default);
}
