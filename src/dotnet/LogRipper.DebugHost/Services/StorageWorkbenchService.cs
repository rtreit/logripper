using Grpc.Core;
using LogRipper.DebugHost.Models;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

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
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(workedCallsign);

        var sampleQso = _sampleProtoFactory.CreateQsoRecord(workedCallsign);
        LogQsoResponse? logResponse = null;
        GetQsoResponse? loadedResponse = null;
        DeleteQsoResponse? deleteResponse = null;
        GetSyncStatusResponse? syncStatus = null;
        var listedQsos = new List<QsoRecord>();
        var deleteVerified = false;

        try
        {
            using var channel = _clientFactory.CreateChannel();
            var client = new LogbookService.LogbookServiceClient(channel);

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

            var errorMessage = !retainRecord && !deleteVerified
                ? "Delete verification failed. GetQso still returned the sample record after delete."
                : null;

            return new StorageSmokeTestResult(
                sampleQso,
                logResponse,
                loadedResponse,
                listedQsos,
                syncStatus,
                deleteResponse,
                retainRecord,
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
                listedQsos,
                syncStatus,
                deleteResponse,
                retainRecord,
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
                listedQsos,
                syncStatus,
                deleteResponse,
                retainRecord,
                deleteVerified,
                ex.Message,
                DateTimeOffset.UtcNow);
        }
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
