using System.Diagnostics.CodeAnalysis;
using BenchmarkDotNet.Attributes;
using Grpc.Net.Client;
using LogRipper.Services;

namespace LogRipper.DebugHost.Benchmarks;

[Config(typeof(DebugHostBenchmarkConfig))]
[MemoryDiagnoser]
[SuppressMessage(
    "Design",
    "CA1515:Consider making public types internal",
    Justification = "BenchmarkDotNet benchmark types are discovered and activated via reflection.")]
public class EngineStartupBenchmarks : EngineBenchmarkBase
{
    private GrpcChannel? _warmChannel;
    private GrpcChannel? _warmTunedChannel;

    [GlobalSetup]
    public async Task SetupAsync()
    {
        await ValidateEndpointAsync();

        // Pre-warm channels so warm-channel benchmarks measure steady-state cost
        _warmChannel = CreateChannel();
        var client1 = new LogbookService.LogbookServiceClient(_warmChannel);
        _ = await client1.GetSyncStatusAsync(new GetSyncStatusRequest()).ResponseAsync;

        _warmTunedChannel = CreateTunedChannel();
        var client2 = new LogbookService.LogbookServiceClient(_warmTunedChannel);
        _ = await client2.GetSyncStatusAsync(new GetSyncStatusRequest()).ResponseAsync;
    }

    [GlobalCleanup]
    public void DisposeChannels()
    {
        _warmChannel?.Dispose();
        _warmTunedChannel?.Dispose();
    }

    // --- Original baselines (cold channel per group) ---

    [Benchmark(Baseline = true, Description = "Current: 3 channels, sequential RPCs")]
    public async Task CurrentStartupAsync()
    {
        using (var setupChannel = CreateChannel())
        {
            await GetSetupStatusAsync(setupChannel);
        }

        using (var stationChannel = CreateChannel())
        {
            await RefreshStationProfilesAsync(stationChannel);
        }

        using (var runtimeChannel = CreateChannel())
        {
            await GetRuntimeConfigAsync(runtimeChannel);
        }
    }

    [Benchmark(Description = "Single channel, sequential RPCs")]
    public async Task SingleChannelSequentialStartupAsync()
    {
        using var channel = CreateChannel();
        await GetSetupStatusAsync(channel);
        await RefreshStationProfilesAsync(channel);
        await GetRuntimeConfigAsync(channel);
    }

    [Benchmark(Description = "Single channel, parallel RPCs")]
    public async Task SingleChannelParallelStartupAsync()
    {
        using var channel = CreateChannel();
        await RunParallelRpcsAsync(channel);
    }

    // --- New: warm channel reuse (measures steady-state RPC cost) ---

    [Benchmark(Description = "Warm channel, sequential RPCs")]
    public async Task WarmChannelSequentialAsync()
    {
        await GetSetupStatusAsync(_warmChannel!);
        await RefreshStationProfilesAsync(_warmChannel!);
        await GetRuntimeConfigAsync(_warmChannel!);
    }

    [Benchmark(Description = "Warm channel, parallel RPCs")]
    public async Task WarmChannelParallelAsync()
    {
        await RunParallelRpcsAsync(_warmChannel!);
    }

    // --- New: tuned SocketsHttpHandler (keepalive, multi-connection) ---

    [Benchmark(Description = "Tuned handler, sequential RPCs")]
    public async Task TunedHandlerSequentialAsync()
    {
        await GetSetupStatusAsync(_warmTunedChannel!);
        await RefreshStationProfilesAsync(_warmTunedChannel!);
        await GetRuntimeConfigAsync(_warmTunedChannel!);
    }

    [Benchmark(Description = "Tuned handler, parallel RPCs")]
    public async Task TunedHandlerParallelAsync()
    {
        await RunParallelRpcsAsync(_warmTunedChannel!);
    }

    // --- New: cold channel first-call isolation ---

    [Benchmark(Description = "Cold channel first call only")]
    public async Task ColdChannelFirstCallAsync()
    {
        using var channel = CreateChannel();
        var client = new SetupService.SetupServiceClient(channel);
        _ = await client.GetSetupStatusAsync(new GetSetupStatusRequest()).ResponseAsync;
    }

    [Benchmark(Description = "Cold tuned channel first call only")]
    public async Task ColdTunedChannelFirstCallAsync()
    {
        using var channel = CreateTunedChannel();
        var client = new SetupService.SetupServiceClient(channel);
        _ = await client.GetSetupStatusAsync(new GetSetupStatusRequest()).ResponseAsync;
    }

    // --- Helpers ---

    private static async Task RunParallelRpcsAsync(GrpcChannel channel)
    {
        var setupClient = new SetupService.SetupServiceClient(channel);
        var stationClient = new StationProfileService.StationProfileServiceClient(channel);
        var developerClient = new DeveloperControlService.DeveloperControlServiceClient(channel);

        var setupTask = setupClient.GetSetupStatusAsync(new GetSetupStatusRequest()).ResponseAsync;
        var catalogTask = stationClient.ListStationProfilesAsync(new ListStationProfilesRequest()).ResponseAsync;
        var contextTask = stationClient.GetActiveStationContextAsync(new GetActiveStationContextRequest()).ResponseAsync;
        var runtimeTask = developerClient.GetRuntimeConfigAsync(new GetRuntimeConfigRequest()).ResponseAsync;

        await Task.WhenAll(setupTask, catalogTask, contextTask, runtimeTask);
    }

    private static async Task GetSetupStatusAsync(GrpcChannel channel)
    {
        var client = new SetupService.SetupServiceClient(channel);
        _ = await client.GetSetupStatusAsync(new GetSetupStatusRequest()).ResponseAsync;
    }

    private static async Task RefreshStationProfilesAsync(GrpcChannel channel)
    {
        var client = new StationProfileService.StationProfileServiceClient(channel);
        _ = await client.ListStationProfilesAsync(new ListStationProfilesRequest()).ResponseAsync;
        _ = await client.GetActiveStationContextAsync(new GetActiveStationContextRequest()).ResponseAsync;
    }

    private static async Task GetRuntimeConfigAsync(GrpcChannel channel)
    {
        var client = new DeveloperControlService.DeveloperControlServiceClient(channel);
        _ = await client.GetRuntimeConfigAsync(new GetRuntimeConfigRequest()).ResponseAsync;
    }
}
