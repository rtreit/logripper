using Grpc.Core;
using LogRipper.Domain;
using LogRipper.DebugHost.Models;

namespace LogRipper.DebugHost.Services;

internal sealed class LookupWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;

    public LookupWorkbenchService(GrpcClientFactory clientFactory)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);

        _clientFactory = clientFactory;
    }

    public async Task<LookupInvocationResult> RunLookupAsync(LookupRequest request, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        try
        {
            using var channel = _clientFactory.CreateChannel();
            var client = new LogRipper.Services.LookupService.LookupServiceClient(channel);
            var response = await client.LookupAsync(request, cancellationToken: cancellationToken);
            return new LookupInvocationResult(request, [response], null, false, DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new LookupInvocationResult(request, Array.Empty<LookupResult>(), ex.Status.Detail, false, DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new LookupInvocationResult(request, Array.Empty<LookupResult>(), ex.Message, false, DateTimeOffset.UtcNow);
        }
    }

    public async Task<LookupInvocationResult> RunStreamingLookupAsync(LookupRequest request, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(request);

        var responses = new List<LookupResult>();

        try
        {
            using var channel = _clientFactory.CreateChannel();
            var client = new LogRipper.Services.LookupService.LookupServiceClient(channel);
            using var call = client.StreamLookup(request, cancellationToken: cancellationToken);

            await foreach (var response in call.ResponseStream.ReadAllAsync(cancellationToken))
            {
                responses.Add(response);
            }

            return new LookupInvocationResult(request, responses, null, true, DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new LookupInvocationResult(request, responses, ex.Status.Detail, true, DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new LookupInvocationResult(request, responses, ex.Message, true, DateTimeOffset.UtcNow);
        }
    }
}
