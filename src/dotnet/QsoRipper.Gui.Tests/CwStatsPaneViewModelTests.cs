using System;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.ViewModels;

namespace QsoRipper.Gui.Tests;

public class CwStatsPaneViewModelTests
{
    /// <summary>
    /// In-memory <see cref="ICwWpmSampleSource"/> that lets tests drive
    /// <c>SampleReceived</c>, <c>RawLineReceived</c>, and
    /// <c>LockStateChanged</c> directly without spinning up the cw-decoder
    /// subprocess.
    /// </summary>
    private sealed class InMemorySource : ICwWpmSampleSource
    {
        public bool IsRunning => true;
        public CwWpmSample? LatestSample { get; private set; }
        public CwLockState CurrentLockState { get; set; } = CwLockState.Unknown;
        public int AnchorHeardCount { get; private set; }

        public event EventHandler<CwWpmSample>? SampleReceived;
        public event EventHandler? StatusChanged;
        public event EventHandler<string>? RawLineReceived;
        public event EventHandler<CwLockState>? LockStateChanged;

        public void EmitSample(CwWpmSample s)
        {
            LatestSample = s;
            SampleReceived?.Invoke(this, s);
        }

        public void EmitRaw(string line) => RawLineReceived?.Invoke(this, line);

        public void EmitLockState(CwLockState s)
        {
            CurrentLockState = s;
            LockStateChanged?.Invoke(this, s);
        }

        public void Start(string? deviceOverride) => StatusChanged?.Invoke(this, EventArgs.Empty);
        public void MarkAnchorHeard() => AnchorHeardCount++;
        public void Stop() => StatusChanged?.Invoke(this, EventArgs.Empty);
        public void Dispose() { }
    }

    private static void Pump()
        => Avalonia.Threading.Dispatcher.UIThread.RunJobs();

    /// <summary>
    /// Run the test body on the Avalonia dispatcher's owning thread.
    /// Other tests in the suite (e.g. <c>FullQsoCardNavigationTests</c>)
    /// initialize Avalonia headless and bind the dispatcher to a
    /// specific worker thread. Without this wrapper, our tests crash
    /// with "calling thread cannot access this object" when they touch
    /// <c>Dispatcher.UIThread.RunJobs()</c> from xUnit's pool.
    /// </summary>
    private static void OnDispatcher(Action body)
        => Avalonia.Threading.Dispatcher.UIThread.Invoke(body);

    [Fact]
    public void ResetClearsDecodedAndWpmAndPreservesLockBadgeFromSource() => OnDispatcher(() =>
    {
        using var src = new InMemorySource { CurrentLockState = CwLockState.Locked };
        using var vm = new CwStatsPaneViewModel(src);

        src.EmitRaw("{\"type\":\"char\",\"ch\":\"K\"}");
        src.EmitRaw("{\"type\":\"char\",\"ch\":\"7\"}");
        src.EmitSample(new CwWpmSample(DateTimeOffset.UtcNow, 18.4, Epoch: 1));
        Pump();

        Assert.Equal("K7", vm.DecodedText);
        Assert.Equal("18.4 WPM", vm.WpmText);
        Assert.True(vm.IsLocked);

        vm.Reset();
        Pump();

        Assert.Equal(string.Empty, vm.DecodedText);
        Assert.Equal("—", vm.WpmText);
        // Source still reports Locked, so the badge must NOT regress to
        // Waiting — the decoder is still tracking the same signal.
        Assert.True(vm.IsLocked);
        Assert.Equal("● LIVE", vm.LockBadgeText);
    });

    [Fact]
    public void LockStateChangedToHuntingDropsIsLockedAndClearsDecodedText() => OnDispatcher(() =>
    {
        using var src = new InMemorySource { CurrentLockState = CwLockState.Locked };
        using var vm = new CwStatsPaneViewModel(src);

        src.EmitRaw("{\"type\":\"char\",\"ch\":\"C\"}");
        src.EmitRaw("{\"type\":\"char\",\"ch\":\"Q\"}");
        src.EmitSample(new CwWpmSample(DateTimeOffset.UtcNow, 22.0, Epoch: 1));
        Pump();
        Assert.True(vm.IsLocked);
        Assert.Equal("CQ", vm.DecodedText);

        // Lock drops -> IsLocked false, decoded text cleared so the
        // operator doesn't think those chars belong to the next decode
        // burst. WPM is intentionally retained as the "last known"
        // reading and dimmed via the stale style.
        src.EmitLockState(CwLockState.Hunting);
        Pump();

        Assert.False(vm.IsLocked);
        Assert.Equal("○ HUNTING", vm.LockBadgeText);
        Assert.Equal(string.Empty, vm.DecodedText);
        Assert.Equal("22.0 WPM", vm.WpmText);
    });

    [Fact]
    public void LockStateChangedToProbationShowsProbationBadgeAndIsNotLocked() => OnDispatcher(() =>
    {
        using var src = new InMemorySource { CurrentLockState = CwLockState.Hunting };
        using var vm = new CwStatsPaneViewModel(src);

        src.EmitLockState(CwLockState.Probation);
        Pump();

        Assert.False(vm.IsLocked);
        Assert.Equal("◐ PROBATION", vm.LockBadgeText);
        Assert.Equal("PROBATION", vm.ConfidenceText);
    });

    [Fact]
    public void ConstructionWithNullSourceShowsMonitorOffBadge() => OnDispatcher(() =>
    {
        using var vm = new CwStatsPaneViewModel(source: null);

        Assert.False(vm.IsLocked);
        Assert.False(vm.IsLive);
        Assert.Equal("○ MONITOR OFF", vm.LockBadgeText);
        Assert.Equal("—", vm.WpmText);
    });

    [Fact]
    public void ConstructionSeedsLockBadgeFromSourceCurrentState() => OnDispatcher(() =>
    {
        // Operator opens the F9 pane *after* the decoder has already
        // locked on a signal — the badge should reflect reality, not lie
        // about "WAITING" until the next confidence event arrives.
        using var src = new InMemorySource { CurrentLockState = CwLockState.Locked };
        using var vm = new CwStatsPaneViewModel(src);

        Assert.True(vm.IsLocked);
        Assert.Equal("● LIVE", vm.LockBadgeText);
        Assert.Equal("LOCKED", vm.ConfidenceText);
    });

    [Fact]
    public void MarkAnchorHeardCommandSendsManualAnchorAndUpdatesStatus() => OnDispatcher(() =>
    {
        using var src = new InMemorySource { CurrentLockState = CwLockState.Hunting };
        using var vm = new CwStatsPaneViewModel(src);

        vm.MarkAnchorHeardCommand.Execute(null);

        Assert.Equal(1, src.AnchorHeardCount);
        Assert.Equal("Manual anchor armed", vm.StatusText);
    });

    [Fact]
    public void StatusEventUpdatesStatusText() => OnDispatcher(() =>
    {
        using var src = new InMemorySource { CurrentLockState = CwLockState.Hunting };
        using var vm = new CwStatsPaneViewModel(src);

        src.EmitRaw("{\"type\":\"status\",\"message\":\"Manual anchor armed\"}");
        Pump();

        Assert.Equal("Manual anchor armed", vm.StatusText);
    });

    [Fact]
    public void DisposeUnsubscribesFromAllSourceEvents() => OnDispatcher(() =>
    {
        using var src = new InMemorySource { CurrentLockState = CwLockState.Locked };
        var vm = new CwStatsPaneViewModel(src);

        var beforeBadge = vm.LockBadgeText;
        vm.Dispose();

        // After dispose, lock state changes must not mutate the VM.
        src.EmitLockState(CwLockState.Hunting);
        src.EmitSample(new CwWpmSample(DateTimeOffset.UtcNow, 99.9, Epoch: 1));
        src.EmitRaw("{\"type\":\"char\",\"ch\":\"X\"}");
        Pump();

        Assert.Equal(beforeBadge, vm.LockBadgeText);
        Assert.NotEqual("99.9 WPM", vm.WpmText);
    });
}
