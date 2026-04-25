using Microsoft.Extensions.Options;
using QsoRipper.DebugHost.Models;
using QsoRipper.DebugHost.Services;

namespace QsoRipper.DebugHost.Tests;

#pragma warning disable CA1707
public class EngineConnectivityWatcherTests
{
    [Fact]
    public void Initial_state_is_reachable_with_resolved_endpoint()
    {
        using var watcher = CreateWatcher((_, _) => Task.FromResult(new EngineProbeOutcome(true, null)));

        Assert.False(watcher.IsEngineUnreachable);
        Assert.Null(watcher.LastError);
        Assert.False(string.IsNullOrWhiteSpace(watcher.LastEndpoint));
    }

    [Fact]
    public async Task Failed_probe_sets_unreachable_and_fires_state_changed_once()
    {
        using var watcher = CreateWatcher((_, _) => Task.FromResult(new EngineProbeOutcome(false, "boom")));
        var fires = 0;
        watcher.StateChanged += (_, _) => Interlocked.Increment(ref fires);

        await watcher.ProbeNowAsync();

        Assert.True(watcher.IsEngineUnreachable);
        Assert.Equal("boom", watcher.LastError);
        Assert.NotEqual(default, watcher.LastProbeUtc);
        Assert.Equal(1, fires);

        await watcher.ProbeNowAsync();
        Assert.Equal(1, fires);
    }

    [Fact]
    public async Task Recovery_after_failure_flips_state_back_and_fires_again()
    {
        var reachable = false;
        using var watcher = CreateWatcher((_, _) => Task.FromResult(new EngineProbeOutcome(reachable, reachable ? null : "down")));
        var fires = 0;
        watcher.StateChanged += (_, _) => Interlocked.Increment(ref fires);

        await watcher.ProbeNowAsync();
        Assert.True(watcher.IsEngineUnreachable);
        Assert.Equal(1, fires);

        reachable = true;
        await watcher.ProbeNowAsync();

        Assert.False(watcher.IsEngineUnreachable);
        Assert.Null(watcher.LastError);
        Assert.Equal(2, fires);
    }

    [Fact]
    public async Task Probe_delegate_exception_is_captured_as_error()
    {
        using var watcher = CreateWatcher((_, _) => throw new InvalidOperationException("kaboom"));

        await watcher.ProbeNowAsync();

        Assert.True(watcher.IsEngineUnreachable);
        Assert.Equal("kaboom", watcher.LastError);
    }

    [Fact]
    public async Task Probe_receives_resolved_endpoint()
    {
        string? captured = null;
        using var watcher = CreateWatcher((endpoint, _) =>
        {
            captured = endpoint;
            return Task.FromResult(new EngineProbeOutcome(true, null));
        }, endpoint: "http://localhost:54321");

        await watcher.ProbeNowAsync();

        Assert.Equal("http://localhost:54321", captured);
        Assert.Equal("http://localhost:54321", watcher.LastEndpoint);
    }

    private static EngineConnectivityWatcher CreateWatcher(
        EngineConnectivityWatcher.ProbeDelegate probe,
        string endpoint = "http://localhost:60051")
    {
        var options = Options.Create(new DebugWorkbenchOptions
        {
            DefaultEngineEndpoint = endpoint,
            ProbeTimeoutSeconds = 1,
        });
        return new EngineConnectivityWatcher(options, logger: null, probe: probe, interval: TimeSpan.FromMinutes(5));
    }
}
