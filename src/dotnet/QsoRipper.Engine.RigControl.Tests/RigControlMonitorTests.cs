using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl.Tests;

public sealed class RigControlMonitorTests
{
    private sealed class FakeProvider(Func<RigSnapshot> factory) : IRigControlProvider
    {
        public RigSnapshot GetSnapshot() => factory();
    }

    private sealed class FailingProvider(RigControlException exception) : IRigControlProvider
    {
        public RigSnapshot GetSnapshot() => throw exception;
    }

    [Fact]
    public void CurrentSnapshotReturnsCachedWithinThreshold()
    {
        var callCount = 0;
        var provider = new FakeProvider(() =>
        {
            callCount++;
            return new RigSnapshot { FrequencyHz = 14_074_000 };
        });

        var monitor = new RigControlMonitor(provider, TimeSpan.FromSeconds(60));

        var first = monitor.CurrentSnapshot();
        var second = monitor.CurrentSnapshot();

        Assert.Equal(14_074_000UL, first.FrequencyHz);
        Assert.Equal(14_074_000UL, second.FrequencyHz);
        Assert.Equal(RigConnectionStatus.Connected, first.Status);
        Assert.Equal(1, callCount);
    }

    [Fact]
    public void RefreshAlwaysCallsProvider()
    {
        var callCount = 0;
        var provider = new FakeProvider(() =>
        {
            callCount++;
            return new RigSnapshot { FrequencyHz = 14_074_000 };
        });

        var monitor = new RigControlMonitor(provider, TimeSpan.FromSeconds(60));

        monitor.RefreshSnapshot();
        monitor.RefreshSnapshot();

        Assert.Equal(2, callCount);
    }

    [Fact]
    public void CurrentSnapshotRefreshesWhenStale()
    {
        var callCount = 0;
        var provider = new FakeProvider(() =>
        {
            callCount++;
            return new RigSnapshot { FrequencyHz = 14_074_000 };
        });

        // Zero threshold means every call is stale.
        var monitor = new RigControlMonitor(provider, TimeSpan.Zero);

        monitor.CurrentSnapshot();
        monitor.CurrentSnapshot();

        Assert.Equal(2, callCount);
    }

    [Fact]
    public void FailureWithoutCacheReturnsErrorSnapshot()
    {
        var provider = new FailingProvider(
            new RigControlException("connection refused", RigControlErrorKind.Transport));

        var monitor = new RigControlMonitor(provider, TimeSpan.FromSeconds(60));
        var snapshot = monitor.CurrentSnapshot();

        Assert.Equal(RigConnectionStatus.Error, snapshot.Status);
        Assert.Equal("connection refused", snapshot.ErrorMessage);
        Assert.Equal(0UL, snapshot.FrequencyHz);
    }

    [Fact]
    public void FailureWithCacheReturnsStaleCachedWithError()
    {
        var shouldFail = false;
        var provider = new FakeProvider(() =>
        {
            if (shouldFail)
            {
                throw new RigControlException("offline", RigControlErrorKind.Transport);
            }

            return new RigSnapshot { FrequencyHz = 7_074_000 };
        });

        // Zero threshold so every call refreshes.
        var monitor = new RigControlMonitor(provider, TimeSpan.Zero);

        // First call succeeds and caches.
        var first = monitor.CurrentSnapshot();
        Assert.Equal(7_074_000UL, first.FrequencyHz);
        Assert.Equal(RigConnectionStatus.Connected, first.Status);

        // Now make provider fail.
        shouldFail = true;
        var stale = monitor.CurrentSnapshot();

        Assert.Equal(RigConnectionStatus.Error, stale.Status);
        Assert.Equal(7_074_000UL, stale.FrequencyHz);
        Assert.Equal("offline", stale.ErrorMessage);
    }

    [Fact]
    public void DisabledProviderReturnsDisabledStatus()
    {
        var provider = new FailingProvider(
            new RigControlException("not configured", RigControlErrorKind.Disabled));

        var monitor = new RigControlMonitor(provider, TimeSpan.FromSeconds(60));
        var snapshot = monitor.CurrentSnapshot();

        Assert.Equal(RigConnectionStatus.Disabled, snapshot.Status);
        Assert.Equal("not configured", snapshot.ErrorMessage);
    }

    [Fact]
    public void SuccessSnapshotHasSampledAt()
    {
        var before = DateTimeOffset.UtcNow;
        var provider = new FakeProvider(() => new RigSnapshot { FrequencyHz = 14_074_000 });
        var monitor = new RigControlMonitor(provider, TimeSpan.FromSeconds(60));

        var snapshot = monitor.CurrentSnapshot();

        Assert.NotNull(snapshot.SampledAt);
        var sampledAt = snapshot.SampledAt.ToDateTimeOffset();
        Assert.True(sampledAt >= before);
        Assert.True(sampledAt <= DateTimeOffset.UtcNow.AddSeconds(1));
    }

    [Fact]
    public void SuccessSnapshotClearsErrorMessage()
    {
        var provider = new FakeProvider(() => new RigSnapshot
        {
            FrequencyHz = 14_074_000,
            ErrorMessage = "should be cleared",
        });

        var monitor = new RigControlMonitor(provider, TimeSpan.FromSeconds(60));
        var snapshot = monitor.CurrentSnapshot();

        Assert.False(snapshot.HasErrorMessage);
    }
}
