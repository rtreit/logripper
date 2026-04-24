using System;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.Tests;

public sealed class CwQsoWpmAggregatorTests
{
    private static readonly DateTimeOffset BaseTime =
        new(2024, 1, 1, 0, 0, 0, TimeSpan.Zero);

    private sealed class FakeSource : ICwWpmSampleSource
    {
        public bool IsRunning => false;
        public CwWpmSample? LatestSample { get; private set; }
        public event EventHandler<CwWpmSample>? SampleReceived;
        public event EventHandler? StatusChanged;
#pragma warning disable CS0067 // unused in tests
        public event EventHandler<string>? RawLineReceived;
#pragma warning restore CS0067

        public void Emit(CwWpmSample sample)
        {
            LatestSample = sample;
            SampleReceived?.Invoke(this, sample);
        }

        public void Start(string? deviceOverride) => StatusChanged?.Invoke(this, EventArgs.Empty);
        public void Stop() => StatusChanged?.Invoke(this, EventArgs.Empty);
        public void Dispose() { }
    }

    [Fact]
    public void GetMeanWpmReturnsNullWhenNoSamples()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src);

        Assert.Null(agg.GetMeanWpm(BaseTime, BaseTime.AddMinutes(1)));
    }

    [Fact]
    public void GetMeanWpmWeightsBySegmentDuration()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(60));

        // 20 wpm held for 10s, then 30 wpm held for 30s.
        // Time-weighted mean = (20*10 + 30*30) / 40 = 27.5
        agg.IngestForTest(new CwWpmSample(BaseTime, 20.0, Epoch: 1));
        agg.IngestForTest(new CwWpmSample(BaseTime.AddSeconds(10), 30.0, Epoch: 1));

        var mean = agg.GetMeanWpm(BaseTime, BaseTime.AddSeconds(40));

        Assert.NotNull(mean);
        Assert.Equal(27.5, mean!.Value, precision: 3);
    }

    [Fact]
    public void GetMeanWpmAppliesMaxSampleHold()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(5));

        // Single 25 wpm sample at t=0; window is 30s. With a 5s hold it should
        // contribute weight 5, giving a mean of exactly 25.
        agg.IngestForTest(new CwWpmSample(BaseTime, 25.0, Epoch: 1));

        var mean = agg.GetMeanWpm(BaseTime, BaseTime.AddSeconds(30));

        Assert.NotNull(mean);
        Assert.Equal(25.0, mean!.Value, precision: 3);
    }

    [Fact]
    public void GetMeanWpmNextSampleTerminatesPreviousEvenAcrossEpoch()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(60));

        // Sample 1 (epoch 1) at t=0; sample 2 (epoch 2 = restart) at t=10.
        // The next sample terminates sample 1's hold even though the epoch
        // differs — a restart must never extend a stale reading.
        // Window [t+0, t+20]:
        //   sample 1: 10s @ 20  (clipped at sample 2's arrival)
        //   sample 2: 10s @ 30  (clipped at window end)
        // Time-weighted mean = (20*10 + 30*10) / 20 = 25.
        agg.IngestForTest(new CwWpmSample(BaseTime, 20.0, Epoch: 1));
        agg.IngestForTest(new CwWpmSample(BaseTime.AddSeconds(10), 30.0, Epoch: 2));

        var mean = agg.GetMeanWpm(BaseTime, BaseTime.AddSeconds(20));

        Assert.NotNull(mean);
        Assert.Equal(25.0, mean!.Value, precision: 3);
    }

    [Fact]
    public void GetMeanWpmClipsSamplesOutsideWindow()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(60));

        // Sample at t=0, weight should clip to the window starting at t=5.
        // Hold ends at t=60. Window is [t+5, t+15] ⇒ 10s @ 40.
        agg.IngestForTest(new CwWpmSample(BaseTime, 40.0, Epoch: 1));

        var mean = agg.GetMeanWpm(BaseTime.AddSeconds(5), BaseTime.AddSeconds(15));

        Assert.NotNull(mean);
        Assert.Equal(40.0, mean!.Value, precision: 3);
    }

    [Fact]
    public void GetMeanWpmReturnsNullWhenSamplesAllOutsideWindow()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(2));

        // Sample at t=0, hold expires at t=2. Window starts at t=10.
        agg.IngestForTest(new CwWpmSample(BaseTime, 20.0, Epoch: 1));

        Assert.Null(agg.GetMeanWpm(BaseTime.AddSeconds(10), BaseTime.AddSeconds(20)));
    }

    [Fact]
    public void GetMeanWpmReturnsNullForInvertedOrZeroWindow()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src);
        agg.IngestForTest(new CwWpmSample(BaseTime, 20.0, Epoch: 1));

        Assert.Null(agg.GetMeanWpm(BaseTime, BaseTime));
        Assert.Null(agg.GetMeanWpm(BaseTime.AddSeconds(10), BaseTime));
    }

    [Fact]
    public void GetMeanWpmIgnoresNonPositiveOrNonFiniteSamples()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxSampleHoldDuration: TimeSpan.FromSeconds(10));

        agg.IngestForTest(new CwWpmSample(BaseTime, 0.0, Epoch: 1));
        agg.IngestForTest(new CwWpmSample(BaseTime.AddSeconds(1), double.NaN, Epoch: 1));
        agg.IngestForTest(new CwWpmSample(BaseTime.AddSeconds(2), -5.0, Epoch: 1));
        agg.IngestForTest(new CwWpmSample(BaseTime.AddSeconds(3), 22.0, Epoch: 1));

        var mean = agg.GetMeanWpm(BaseTime, BaseTime.AddSeconds(30));

        Assert.NotNull(mean);
        Assert.Equal(22.0, mean!.Value, precision: 3);
    }

    [Fact]
    public void EvictsOldestSamplesOverCap()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src, maxRetainedSamples: 3);

        for (int i = 0; i < 10; i++)
        {
            agg.IngestForTest(new CwWpmSample(BaseTime.AddSeconds(i), 20.0 + i, Epoch: 1));
        }

        Assert.Equal(3, agg.SampleCount);
    }

    [Fact]
    public void ClearDropsAllSamples()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoWpmAggregator(src);
        agg.IngestForTest(new CwWpmSample(BaseTime, 20.0, Epoch: 1));
        Assert.Equal(1, agg.SampleCount);

        agg.Clear();

        Assert.Equal(0, agg.SampleCount);
    }
}
