using Grpc.Core;
using QsoRipper.DebugHost.Models;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class StorageWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;
    private readonly SampleProtoFactory _sampleProtoFactory;

    public StorageWorkbenchService(GrpcClientFactory clientFactory, SampleProtoFactory sampleProtoFactory)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);
        ArgumentNullException.ThrowIfNull(sampleProtoFactory);

        _clientFactory = clientFactory;
        _sampleProtoFactory = sampleProtoFactory;
    }

    public async Task<StorageSmokeTestResult> RunSmokeTestAsync(
        string workedCallsign,
        bool retainRecord,
        StationProfile? activeStationProfile = null,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(workedCallsign);

        var sampleQso = _sampleProtoFactory.CreateQsoRecord(workedCallsign);
        ApplyStationProfile(sampleQso, activeStationProfile);
        LogQsoResponse? logResponse = null;
        GetQsoResponse? loadedResponse = null;
        UpdateQsoResponse? updateResponse = null;
        GetQsoResponse? updatedResponse = null;
        DeleteQsoResponse? deleteResponse = null;
        GetSyncStatusResponse? syncStatus = null;
        var listedQsos = new List<QsoRecord>();
        var updateVerified = false;
        var deleteVerified = false;

        try
        {
            var client = _clientFactory.CreateLogbookClient();

            logResponse = await client.LogQsoAsync(
                new LogQsoRequest
                {
                    Qso = sampleQso,
                    SyncToQrz = false
                },
                cancellationToken: cancellationToken);

            loadedResponse = await client.GetQsoAsync(
                new GetQsoRequest { LocalId = logResponse.LocalId },
                cancellationToken: cancellationToken);

            var updatedQso = BuildUpdatedQso(
                loadedResponse.Qso ?? throw new InvalidOperationException("GetQso returned a response without a qso payload."));
            updateResponse = await client.UpdateQsoAsync(
                new UpdateQsoRequest
                {
                    Qso = updatedQso,
                    SyncToQrz = false
                },
                cancellationToken: cancellationToken);

            updatedResponse = await client.GetQsoAsync(
                new GetQsoRequest { LocalId = logResponse.LocalId },
                cancellationToken: cancellationToken);
            updateVerified = updateResponse.Success && VerifyUpdatedQso(updatedResponse.Qso, updatedQso);

            using var listCall = client.ListQsos(
                new ListQsosRequest
                {
                    CallsignFilter = sampleQso.WorkedCallsign,
                    Limit = 25,
                    Sort = QsoSortOrder.NewestFirst
                },
                cancellationToken: cancellationToken);

            await foreach (var response in listCall.ResponseStream.ReadAllAsync(cancellationToken))
            {
                var qso = response.Qso ?? throw new InvalidOperationException("ListQsos returned a response without a qso payload.");
                listedQsos.Add(qso);
            }

            syncStatus = await client.GetSyncStatusAsync(
                new GetSyncStatusRequest(),
                cancellationToken: cancellationToken);

            if (!retainRecord)
            {
                deleteResponse = await client.DeleteQsoAsync(
                    new DeleteQsoRequest
                    {
                        LocalId = logResponse.LocalId,
                        DeleteFromQrz = false
                    },
                    cancellationToken: cancellationToken);
                deleteVerified = await ConfirmDeleteAsync(client, logResponse.LocalId, cancellationToken);
            }

            var errorMessages = new List<string>();
            if (updateResponse is { Success: false })
            {
                errorMessages.Add(
                    $"UpdateQso returned failure: {updateResponse.Error ?? "(no detail was returned by the engine)"}");
            }
            else if (!updateVerified)
            {
                errorMessages.Add("Update verification failed. GetQso did not return the updated sample record.");
            }

            if (!retainRecord)
            {
                if (deleteResponse is { Success: false })
                {
                    errorMessages.Add(
                        $"DeleteQso returned failure: {deleteResponse.Error ?? "(no detail was returned by the engine)"}");
                }
                else if (!deleteVerified)
                {
                    errorMessages.Add("Delete verification failed. GetQso still returned the sample record after delete.");
                }
            }

            var errorMessage = errorMessages.Count == 0
                ? null
                : string.Join(" ", errorMessages);

            return new StorageSmokeTestResult(
                sampleQso,
                logResponse,
                loadedResponse,
                updateResponse,
                updatedResponse,
                listedQsos,
                syncStatus,
                deleteResponse,
                retainRecord,
                updateVerified,
                deleteVerified,
                errorMessage,
                DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new StorageSmokeTestResult(
                sampleQso,
                logResponse,
                loadedResponse,
                updateResponse,
                updatedResponse,
                listedQsos,
                syncStatus,
                deleteResponse,
                retainRecord,
                updateVerified,
                deleteVerified,
                ex.Status.Detail,
                DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new StorageSmokeTestResult(
                sampleQso,
                logResponse,
                loadedResponse,
                updateResponse,
                updatedResponse,
                listedQsos,
                syncStatus,
                deleteResponse,
                retainRecord,
                updateVerified,
                deleteVerified,
                ex.Message,
                DateTimeOffset.UtcNow);
        }
    }

    public async Task<PurgeDeletedQsosResponse> PurgeDeletedQsosAsync(CancellationToken cancellationToken = default)
    {
        var client = _clientFactory.CreateLogbookClient();

        return await client.PurgeDeletedQsosAsync(
            new PurgeDeletedQsosRequest { Confirm = true },
            cancellationToken: cancellationToken);
    }

    internal static QsoRecord BuildUpdatedQso(QsoRecord source)
    {
        ArgumentNullException.ThrowIfNull(source);

        var updated = source.Clone();
        updated.Comment = $"DebugHost storage smoke update {DateTime.UtcNow:O}";
        return updated;
    }

    private static void ApplyStationProfile(QsoRecord qso, StationProfile? profile)
    {
        if (profile is null)
        {
            return;
        }

        qso.StationCallsign = profile.StationCallsign;
        qso.StationSnapshot = new StationSnapshot
        {
            ProfileName = profile.ProfileName,
            StationCallsign = profile.StationCallsign,
            OperatorCallsign = profile.OperatorCallsign,
            OperatorName = profile.OperatorName,
            Grid = profile.Grid,
            County = profile.County,
            State = profile.State,
            Country = profile.Country,
            Dxcc = profile.Dxcc,
            CqZone = profile.CqZone,
            ItuZone = profile.ItuZone,
            Latitude = profile.Latitude,
            Longitude = profile.Longitude,
            ArrlSection = profile.ArrlSection
        };
    }

    private static bool VerifyUpdatedQso(QsoRecord? persisted, QsoRecord expected)
    {
        ArgumentNullException.ThrowIfNull(expected);

        return persisted is not null
            && string.Equals(persisted.LocalId, expected.LocalId, StringComparison.Ordinal)
            && string.Equals(persisted.Comment, expected.Comment, StringComparison.Ordinal);
    }

    private static async Task<bool> ConfirmDeleteAsync(
        LogbookService.LogbookServiceClient client,
        string localId,
        CancellationToken cancellationToken)
    {
        try
        {
            _ = await client.GetQsoAsync(
                new GetQsoRequest { LocalId = localId },
                cancellationToken: cancellationToken);
            return false;
        }
        catch (RpcException ex) when (ex.StatusCode == StatusCode.NotFound)
        {
            return true;
        }
    }
}
