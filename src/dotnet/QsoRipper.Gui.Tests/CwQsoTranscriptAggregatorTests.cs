using System;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.Tests;

public sealed class CwQsoTranscriptAggregatorTests
{
    private sealed class FakeSource : ICwWpmSampleSource
    {
        public bool IsRunning => false;
        public CwWpmSample? LatestSample => null;
        public CwLockState CurrentLockState => CwLockState.Locked;
#pragma warning disable CS0067 // unused in tests
        public event EventHandler<CwWpmSample>? SampleReceived;
        public event EventHandler? StatusChanged;
        public event EventHandler<CwLockState>? LockStateChanged;
#pragma warning restore CS0067
        public event EventHandler<string>? RawLineReceived;

        public void Emit(string line) => RawLineReceived?.Invoke(this, line);
        public void Start(string? deviceOverride) { }
        public void MarkAnchorHeard() { }
        public void Stop() { }
        public void Dispose() { }
    }

    [Fact]
    public void ReturnsNullWhenNoFragments()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoTranscriptAggregator(src);

        var now = DateTimeOffset.UtcNow;
        Assert.Null(agg.GetTranscript(now, now.AddMinutes(1)));
    }

    [Fact]
    public void AccumulatesCharsWordsAndGarbledFromRawNdjson()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoTranscriptAggregator(src);

        var start = DateTimeOffset.UtcNow.AddSeconds(-1);

        src.Emit("{\"type\":\"char\",\"ch\":\"C\",\"morse\":\"-.-.\"}");
        src.Emit("{\"type\":\"char\",\"ch\":\"Q\",\"morse\":\"--.-\"}");
        src.Emit("{\"type\":\"word\"}");
        src.Emit("{\"type\":\"garbled\",\"symbol\":\"..-...-\"}");
        src.Emit("{\"type\":\"char\",\"ch\":\"D\",\"morse\":\"-..\"}");
        src.Emit("{\"type\":\"char\",\"ch\":\"E\",\"morse\":\".\"}");

        var transcript = agg.GetTranscript(start, DateTimeOffset.UtcNow.AddSeconds(1));

        Assert.Equal("CQ ?DE", transcript);
    }

    [Fact]
    public void NormalizeCollapsesWhitespaceAndTrimsTrailing()
    {
        var sb = new System.Text.StringBuilder();
        sb.Append(" CQ   ").Append("DE  K1ABC ");

        var normalized = CwQsoTranscriptAggregator.Normalize(sb);

        Assert.Equal("CQ DE K1ABC", normalized);
    }

    [Fact]
    public void NormalizeStripsControlCharacters()
    {
        var sb = new System.Text.StringBuilder();
        sb.Append("AB").Append('\u0007').Append('C');

        var normalized = CwQsoTranscriptAggregator.Normalize(sb);

        Assert.Equal("ABC", normalized);
    }

    [Fact]
    public void IgnoresUnknownEventTypesAndMalformedJson()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoTranscriptAggregator(src);

        var start = DateTimeOffset.UtcNow.AddSeconds(-1);

        src.Emit("not json at all");
        src.Emit("{\"type\":\"power\",\"snr\":1.0}");
        src.Emit("{\"type\":\"pitch\",\"hz\":700}");
        src.Emit("{}");
        src.Emit("{\"type\":\"char\"}"); // missing ch
        src.Emit("{\"type\":\"char\",\"ch\":\"\"}"); // empty ch
        src.Emit("{\"type\":\"char\",\"ch\":\"K\"}");

        var transcript = agg.GetTranscript(start, DateTimeOffset.UtcNow.AddSeconds(1));

        Assert.Equal("K", transcript);
    }

    [Fact]
    public void OutOfWindowFragmentsExcluded()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoTranscriptAggregator(src);

        // Emit before window
        src.Emit("{\"type\":\"char\",\"ch\":\"X\"}");

        // Capture window start AFTER first emit
        System.Threading.Thread.Sleep(20);
        var windowStart = DateTimeOffset.UtcNow;
        System.Threading.Thread.Sleep(20);

        src.Emit("{\"type\":\"char\",\"ch\":\"Y\"}");

        var windowEnd = DateTimeOffset.UtcNow.AddSeconds(1);

        var transcript = agg.GetTranscript(windowStart, windowEnd);

        Assert.Equal("Y", transcript);
    }

    [Fact]
    public void HandlerSwallowsExceptionsAndContinuesProcessing()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoTranscriptAggregator(src);

        // Two valid lines around an empty/whitespace one (which the
        // handler should silently drop without affecting subsequent
        // events). Aggregator must remain healthy.
        src.Emit("{\"type\":\"char\",\"ch\":\"A\"}");
        src.Emit("");
        src.Emit("   ");
        src.Emit("{\"type\":\"char\",\"ch\":\"B\"}");

        var transcript = agg.GetTranscript(DateTimeOffset.UtcNow.AddMinutes(-1), DateTimeOffset.UtcNow.AddMinutes(1));

        Assert.Equal("AB", transcript);
    }

    [Fact]
    public void DisposeUnsubscribesFromSource()
    {
        using var src = new FakeSource();
        var agg = new CwQsoTranscriptAggregator(src);

        agg.Dispose();
        // After dispose, further emits should not affect FragmentCount.
        src.Emit("{\"type\":\"char\",\"ch\":\"Z\"}");

        Assert.Equal(0, agg.FragmentCount);
    }

    [Fact]
    public void RetentionCapDropsOldestFragments()
    {
        using var src = new FakeSource();
        using var agg = new CwQsoTranscriptAggregator(src, maxRetainedFragments: 4);

        for (int i = 0; i < 10; i++)
        {
            src.Emit("{\"type\":\"char\",\"ch\":\"X\"}");
        }

        Assert.Equal(4, agg.FragmentCount);
    }
}
