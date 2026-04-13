using System;
using System.Threading;
using System.Threading.Tasks;
using Grpc.Net.Client;
using QsoRipper.Services;

namespace QsoRipper.Gui.Services;

/// <summary>
/// Thin wrapper over gRPC SetupService client for the GUI layer.
/// </summary>
internal sealed class EngineGrpcService : IDisposable
{
    private readonly GrpcChannel _channel;
    private readonly SetupService.SetupServiceClient _setupClient;

    public EngineGrpcService(GrpcChannel channel)
    {
        _channel = channel;
        _setupClient = new SetupService.SetupServiceClient(channel);
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

    public void Dispose()
    {
        _channel.Dispose();
    }
}
