using Grpc.Core;
using QsoRipper.DebugHost.Models;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class QsoViewerService
{
    private readonly GrpcClientFactory _clientFactory;

    public QsoViewerService(GrpcClientFactory clientFactory)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);

        _clientFactory = clientFactory;
    }

    public async Task<QsoViewerResult> ListQsosAsync(
        ListQsosRequest request,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        var records = new List<QsoRecord>();

        try
        {
            var client = _clientFactory.CreateLogbookClient();

            using var call = client.ListQsos(request, cancellationToken: cancellationToken);

            await foreach (var response in call.ResponseStream.ReadAllAsync(cancellationToken))
            {
                if (response.Qso is not null)
                {
                    records.Add(response.Qso);
                }
            }

            return new QsoViewerResult(
                records,
                (uint)records.Count,
                ErrorMessage: null,
                DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new QsoViewerResult(
                records,
                (uint)records.Count,
                ex.Status.Detail,
                DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new QsoViewerResult(
                records,
                (uint)records.Count,
                ex.Message,
                DateTimeOffset.UtcNow);
        }
    }

    public async Task<(QsoRecord? Qso, string? ErrorMessage)> GetQsoAsync(
        string localId,
        CancellationToken cancellationToken = default)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        try
        {
            var client = _clientFactory.CreateLogbookClient();

            var response = await client.GetQsoAsync(
                new GetQsoRequest { LocalId = localId },
                cancellationToken: cancellationToken);

            return (response.Qso, null);
        }
        catch (RpcException ex)
        {
            return (null, ex.Status.Detail);
        }
        catch (OperationCanceledException ex)
        {
            return (null, ex.Message);
        }
    }
}
