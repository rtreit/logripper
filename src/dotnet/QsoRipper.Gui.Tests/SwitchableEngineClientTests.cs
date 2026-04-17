using Grpc.Core;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Gui.Services;
using QsoRipper.Services;

namespace QsoRipper.Gui.Tests;

public sealed class SwitchableEngineClientTests
{
    [Fact]
    public async Task SwitchAsyncReplacesClientWhenProbeSucceeds()
    {
        var rustProfile = EngineCatalog.GetProfile(KnownEngineProfiles.LocalRust);
        var dotnetProfile = EngineCatalog.GetProfile(KnownEngineProfiles.LocalDotNet);
        var first = new FakeEngineClient("rust");
        var second = new FakeEngineClient("dotnet");
        var clients = new Dictionary<string, FakeEngineClient>(StringComparer.OrdinalIgnoreCase)
        {
            ["http://engine-a"] = first,
            ["http://engine-b"] = second,
        };
        using var switchable = new SwitchableEngineClient(
            rustProfile,
            "http://engine-a",
            endpoint => clients[endpoint]);

        var initialQsos = await switchable.ListRecentQsosAsync();
        Assert.Equal("rust", Assert.Single(initialQsos).LocalId);

        var result = await switchable.SwitchAsync(dotnetProfile, "http://engine-b");

        Assert.True(result.Success);
        Assert.Equal(dotnetProfile.ProfileId, switchable.CurrentProfile.ProfileId);
        Assert.Equal("http://engine-b", switchable.CurrentEndpoint);
        var switchedQsos = await switchable.ListRecentQsosAsync();
        Assert.Equal("dotnet", Assert.Single(switchedQsos).LocalId);
        Assert.Equal(0, first.DisposeCount);
        Assert.Equal(0, second.DisposeCount);
    }

    [Fact]
    public async Task SwitchAsyncKeepsCurrentClientWhenProbeFails()
    {
        var rustProfile = EngineCatalog.GetProfile(KnownEngineProfiles.LocalRust);
        var dotnetProfile = EngineCatalog.GetProfile(KnownEngineProfiles.LocalDotNet);
        var first = new FakeEngineClient("rust");
        var failedCandidate = new FakeEngineClient("dotnet") { ProbeException = new RpcException(new Status(StatusCode.Unavailable, "offline")) };
        var clients = new Dictionary<string, FakeEngineClient>(StringComparer.OrdinalIgnoreCase)
        {
            ["http://engine-a"] = first,
            ["http://engine-b"] = failedCandidate,
        };
        using var switchable = new SwitchableEngineClient(
            rustProfile,
            "http://engine-a",
            endpoint => clients[endpoint]);

        var result = await switchable.SwitchAsync(dotnetProfile, "http://engine-b");

        Assert.False(result.Success);
        Assert.Equal(rustProfile.ProfileId, switchable.CurrentProfile.ProfileId);
        Assert.Equal("http://engine-a", switchable.CurrentEndpoint);
        var currentQsos = await switchable.ListRecentQsosAsync();
        Assert.Equal("rust", Assert.Single(currentQsos).LocalId);
        Assert.Equal(1, failedCandidate.DisposeCount);
    }

    private sealed class FakeEngineClient(string localId) : IEngineClient, IDisposable
    {
        public int DisposeCount { get; private set; }

        public Exception? ProbeException { get; init; }

        public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<ValidateSetupStepResponse> ValidateStepAsync(
            ValidateSetupStepRequest request,
            CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(
            string username,
            string password,
            CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(
            string apiKey,
            CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default) =>
            Task.FromResult<IReadOnlyList<QsoRecord>>([new QsoRecord { LocalId = localId }]);

        public Task<UpdateQsoResponse> UpdateQsoAsync(
            QsoRecord qso,
            bool syncToQrz = false,
            CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default)
        {
            if (ProbeException is not null)
            {
                throw ProbeException;
            }

            return Task.FromResult(new GetSyncStatusResponse());
        }

        public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<DeleteQsoResponse> DeleteQsoAsync(
            string localId,
            bool deleteFromQrz = false,
            CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default) =>
            throw new NotImplementedException();

        public void Dispose()
        {
            DisposeCount++;
        }
    }
}
