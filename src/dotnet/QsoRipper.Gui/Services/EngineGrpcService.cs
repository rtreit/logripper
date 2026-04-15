using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Thin wrapper over gRPC SetupService client for the GUI layer.
/// </summary>
internal sealed class EngineGrpcService : IEngineClient, IDisposable
{
    private readonly GrpcChannel _channel;
    private readonly SetupService.SetupServiceClient _setupClient;
    private readonly LogbookService.LogbookServiceClient _logbookClient;
    private readonly LookupService.LookupServiceClient _lookupClient;

    public EngineGrpcService(GrpcChannel channel)
    {
        _channel = channel;
        _setupClient = new SetupService.SetupServiceClient(channel);
        _logbookClient = new LogbookService.LogbookServiceClient(channel);
        _lookupClient = new LookupService.LookupServiceClient(channel);
    }

    public async Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default)
    {
        return await _setupClient.GetSetupWizardStateAsync(
            new GetSetupWizardStateRequest(), cancellationToken: ct);
    }

    public async Task<ValidateSetupStepResponse> ValidateStepAsync(
        ValidateSetupStepRequest request,
        CancellationToken ct = default)
    {
        return await _setupClient.ValidateSetupStepAsync(request, cancellationToken: ct);
    }

    public async Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(
        string username,
        string password,
        CancellationToken ct = default)
    {
        return await _setupClient.TestQrzCredentialsAsync(
            new TestQrzCredentialsRequest
            {
                QrzXmlUsername = username,
                QrzXmlPassword = password,
            },
            cancellationToken: ct);
    }

    public async Task<SaveSetupResponse> SaveSetupAsync(
        SaveSetupRequest request,
        CancellationToken ct = default)
    {
        return await _setupClient.SaveSetupAsync(request, cancellationToken: ct);
    }

    public async Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default)
    {
        return await _setupClient.GetSetupStatusAsync(
            new GetSetupStatusRequest(), cancellationToken: ct);
    }

    public async Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(
        string apiKey,
        CancellationToken ct = default)
    {
        return await _setupClient.TestQrzLogbookCredentialsAsync(
            new TestQrzLogbookCredentialsRequest
            {
                ApiKey = apiKey,
            },
            cancellationToken: ct);
    }

    public async Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default)
    {
        ArgumentOutOfRangeException.ThrowIfNegativeOrZero(limit);

        var recentQsos = new List<QsoRecord>();
        using var call = _logbookClient.ListQsos(
            new ListQsosRequest
            {
                Limit = (uint)limit,
                Sort = QsoSortOrder.NewestFirst
            },
            cancellationToken: ct);

        await foreach (var response in call.ResponseStream.ReadAllAsync(ct))
        {
            if (response.Qso is not null)
            {
                recentQsos.Add(response.Qso);
            }
        }

        return recentQsos;
    }

    public async Task<UpdateQsoResponse> UpdateQsoAsync(
        QsoRecord qso,
        bool syncToQrz = false,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(qso);

        return await _logbookClient.UpdateQsoAsync(
            new UpdateQsoRequest
            {
                Qso = qso,
                SyncToQrz = syncToQrz
            },
            cancellationToken: ct);
    }

    public async Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default)
    {
        using var call = _logbookClient.SyncWithQrz(
            new SyncWithQrzRequest(),
            cancellationToken: ct);

        SyncWithQrzResponse? last = null;
        await foreach (var response in call.ResponseStream.ReadAllAsync(ct))
        {
            last = response;
        }

        return last ?? new SyncWithQrzResponse();
    }

    public async Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default)
    {
        return await _logbookClient.GetSyncStatusAsync(
            new GetSyncStatusRequest(), cancellationToken: ct);
    }

    public async Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        return await _lookupClient.LookupAsync(
            new LookupRequest { Callsign = callsign },
            cancellationToken: ct);
    }

    public async Task<DeleteQsoResponse> DeleteQsoAsync(
        string localId,
        bool deleteFromQrz = false,
        CancellationToken ct = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        return await _logbookClient.DeleteQsoAsync(
            new DeleteQsoRequest
            {
                LocalId = localId,
                DeleteFromQrz = deleteFromQrz
            },
            cancellationToken: ct);
    }

    public void Dispose()
    {
        _channel.Dispose();
    }
}
