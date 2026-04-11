using Grpc.Core;
using LogRipper.DebugHost.Models;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Services;

internal sealed class LookupWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;
    private const string UnaryMode = "Unary lookup";
    private const string StreamingMode = "Streaming lookup";
    private const string CacheMode = "Cache lookup";

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
            return new LookupInvocationResult(
                request,
                [response.Result ?? new LookupResult()],
                null,
                UnaryMode,
                DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new LookupInvocationResult(request, Array.Empty<LookupResult>(), ex.Status.Detail, UnaryMode, DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new LookupInvocationResult(request, Array.Empty<LookupResult>(), ex.Message, UnaryMode, DateTimeOffset.UtcNow);
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
            using var call = client.StreamLookup(
                new StreamLookupRequest
                {
                    Callsign = request.Callsign,
                    SkipCache = request.SkipCache
                },
                cancellationToken: cancellationToken);

            await foreach (var response in call.ResponseStream.ReadAllAsync(cancellationToken))
            {
                responses.Add(response.Result ?? new LookupResult());
            }

            return new LookupInvocationResult(request, responses, null, StreamingMode, DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new LookupInvocationResult(request, responses, ex.Status.Detail, StreamingMode, DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new LookupInvocationResult(request, responses, ex.Message, StreamingMode, DateTimeOffset.UtcNow);
        }
    }

    public async Task<LookupInvocationResult> RunCachedLookupAsync(string callsign, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(callsign);

        var normalizedCallsign = NormalizeCallsign(callsign);
        var syntheticRequest = new LookupRequest
        {
            Callsign = normalizedCallsign,
            SkipCache = false
        };

        try
        {
            using var channel = _clientFactory.CreateChannel();
            var client = new LogRipper.Services.LookupService.LookupServiceClient(channel);
            var response = await client.GetCachedCallsignAsync(
                new GetCachedCallsignRequest { Callsign = normalizedCallsign },
                cancellationToken: cancellationToken);
            return new LookupInvocationResult(
                syntheticRequest,
                [response.Result ?? new LookupResult()],
                null,
                CacheMode,
                DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new LookupInvocationResult(syntheticRequest, Array.Empty<LookupResult>(), ex.Status.Detail, CacheMode, DateTimeOffset.UtcNow);
        }
        catch (OperationCanceledException ex)
        {
            return new LookupInvocationResult(syntheticRequest, Array.Empty<LookupResult>(), ex.Message, CacheMode, DateTimeOffset.UtcNow);
        }
    }

    private static string NormalizeCallsign(string callsign)
    {
        return string.IsNullOrWhiteSpace(callsign)
            ? "K7DBG"
            : callsign.Trim().ToUpperInvariant();
    }
}
