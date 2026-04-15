using Grpc.Core;
using QsoRipper.DebugHost.Models;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class RigControlWorkbenchService
{
    private readonly GrpcClientFactory _clientFactory;

    public RigControlWorkbenchService(GrpcClientFactory clientFactory)
    {
        ArgumentNullException.ThrowIfNull(clientFactory);

        _clientFactory = clientFactory;
    }

    public async Task<RigControlInvocationResult> GetRigStatusAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            var client = CreateClient();
            var response = await client.GetRigStatusAsync(new GetRigStatusRequest(), cancellationToken: cancellationToken);
            return new RigControlInvocationResult("GetRigStatus", true, null, response, DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new RigControlInvocationResult("GetRigStatus", false, ex.Status.Detail, null, DateTimeOffset.UtcNow);
        }
    }

    public async Task<RigControlInvocationResult> GetRigSnapshotAsync(CancellationToken cancellationToken = default)
    {
        try
        {
            var client = CreateClient();
            var response = await client.GetRigSnapshotAsync(new GetRigSnapshotRequest(), cancellationToken: cancellationToken);
            return new RigControlInvocationResult("GetRigSnapshot", true, null, response, DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new RigControlInvocationResult("GetRigSnapshot", false, ex.Status.Detail, null, DateTimeOffset.UtcNow);
        }
    }

    public async Task<RigControlInvocationResult> TestRigConnectionAsync(
        string? host,
        uint? port,
        CancellationToken cancellationToken = default)
    {
        try
        {
            var request = new TestRigConnectionRequest();
            if (!string.IsNullOrWhiteSpace(host))
            {
                request.Host = host;
            }

            if (port.HasValue)
            {
                request.Port = port.Value;
            }

            var client = CreateClient();
            var response = await client.TestRigConnectionAsync(request, cancellationToken: cancellationToken);
            return new RigControlInvocationResult(
                "TestRigConnection",
                response.Success,
                response.HasErrorMessage ? response.ErrorMessage : null,
                response,
                DateTimeOffset.UtcNow);
        }
        catch (RpcException ex)
        {
            return new RigControlInvocationResult("TestRigConnection", false, ex.Status.Detail, null, DateTimeOffset.UtcNow);
        }
    }

    private RigControlService.RigControlServiceClient CreateClient()
    {
        return _clientFactory.CreateRigControlClient();
    }
}
