using QsoRipper.Domain;
using QsoRipper.Engine.Lookup;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class BatchLookupTests
{
    private sealed class FakeCoordinator : ILookupCoordinator
    {
        private int _concurrentCount;
        private int _peakConcurrent;

        public int PeakConcurrent => _peakConcurrent;
        public int TotalLookups;

        public async Task<LookupResult> LookupAsync(string callsign, bool skipCache = false, CancellationToken ct = default)
        {
            var current = Interlocked.Increment(ref _concurrentCount);
            UpdatePeak(current);
            Interlocked.Increment(ref TotalLookups);

            await Task.Delay(10, ct); // Simulate work

            Interlocked.Decrement(ref _concurrentCount);

            return new LookupResult
            {
                State = LookupState.Found,
                QueriedCallsign = callsign.Trim().ToUpperInvariant(),
                Record = new CallsignRecord
                {
                    Callsign = callsign.Trim().ToUpperInvariant(),
                    CrossRef = callsign.Trim().ToUpperInvariant(),
                },
            };
        }

        public Task<LookupResult> GetCachedAsync(string callsign) => throw new NotImplementedException();
        public Task<LookupResult[]> StreamLookupAsync(string callsign, CancellationToken ct = default) => throw new NotImplementedException();

        private void UpdatePeak(int current)
        {
            int peak;
            do
            {
                peak = _peakConcurrent;
                if (current <= peak)
                {
                    break;
                }
            }
            while (Interlocked.CompareExchange(ref _peakConcurrent, current, peak) != peak);
        }
    }

    // ── Empty / single input ────────────────────────────────────────────

    [Fact]
    public async Task ExecuteAsync_EmptyList_ReturnsEmpty()
    {
        var coordinator = new FakeCoordinator();
        var results = await BatchLookupOrchestrator.ExecuteAsync(coordinator, []);
        Assert.Empty(results);
    }

    [Fact]
    public async Task ExecuteAsync_SingleCallsign_ReturnsOneResult()
    {
        var coordinator = new FakeCoordinator();
        var results = await BatchLookupOrchestrator.ExecuteAsync(coordinator, ["W1AW"]);

        Assert.Single(results);
        Assert.Equal(LookupState.Found, results[0].State);
        Assert.Equal("W1AW", results[0].QueriedCallsign);
    }

    // ── Multiple callsigns ──────────────────────────────────────────────

    [Fact]
    public async Task ExecuteAsync_MultipleCallsigns_ReturnsInOrder()
    {
        var coordinator = new FakeCoordinator();
        var callsigns = new[] { "W1AW", "K7ABC", "VE3XYZ" };
        var results = await BatchLookupOrchestrator.ExecuteAsync(coordinator, callsigns);

        Assert.Equal(3, results.Length);
        Assert.Equal("W1AW", results[0].QueriedCallsign);
        Assert.Equal("K7ABC", results[1].QueriedCallsign);
        Assert.Equal("VE3XYZ", results[2].QueriedCallsign);
    }

    [Fact]
    public async Task ExecuteAsync_AllCallsignsAreLookedUp()
    {
        var coordinator = new FakeCoordinator();
        var callsigns = new[] { "AA1A", "BB2B", "CC3C", "DD4D" };
        var results = await BatchLookupOrchestrator.ExecuteAsync(coordinator, callsigns);

        Assert.Equal(4, results.Length);
        Assert.Equal(4, coordinator.TotalLookups);
        Assert.All(results, r => Assert.Equal(LookupState.Found, r.State));
    }

    // ── Concurrency limiting ────────────────────────────────────────────

    [Fact]
    public async Task ExecuteAsync_RespectsMaxConcurrency()
    {
        var coordinator = new FakeCoordinator();
        var callsigns = Enumerable.Range(0, 20).Select(i => $"CALL{i}").ToArray();
        var results = await BatchLookupOrchestrator.ExecuteAsync(
            coordinator, callsigns, maxConcurrency: 3);

        Assert.Equal(20, results.Length);
        Assert.Equal(20, coordinator.TotalLookups);
        Assert.True(
            coordinator.PeakConcurrent <= 3,
            $"Peak concurrency was {coordinator.PeakConcurrent}, expected <= 3");
    }

    [Fact]
    public async Task ExecuteAsync_DefaultConcurrency_IsReasonable()
    {
        var coordinator = new FakeCoordinator();
        var callsigns = Enumerable.Range(0, 15).Select(i => $"CALL{i}").ToArray();
        var results = await BatchLookupOrchestrator.ExecuteAsync(coordinator, callsigns);

        Assert.Equal(15, results.Length);
        Assert.True(
            coordinator.PeakConcurrent <= BatchLookupOrchestrator.DefaultMaxConcurrency,
            $"Peak concurrency {coordinator.PeakConcurrent} exceeded default {BatchLookupOrchestrator.DefaultMaxConcurrency}");
    }

    // ── Cancellation ────────────────────────────────────────────────────

    [Fact]
    public async Task ExecuteAsync_CancellationRespected()
    {
        var coordinator = new FakeCoordinator();
        using var cts = new CancellationTokenSource();
        cts.Cancel();

        await Assert.ThrowsAnyAsync<OperationCanceledException>(
            () => BatchLookupOrchestrator.ExecuteAsync(coordinator, ["W1AW"], ct: cts.Token));
    }

    // ── Null argument guards ────────────────────────────────────────────

    [Fact]
    public async Task ExecuteAsync_NullCoordinator_Throws()
    {
        await Assert.ThrowsAsync<ArgumentNullException>(
            () => BatchLookupOrchestrator.ExecuteAsync(null!, ["W1AW"]));
    }

    [Fact]
    public async Task ExecuteAsync_NullCallsigns_Throws()
    {
        var coordinator = new FakeCoordinator();
        await Assert.ThrowsAsync<ArgumentNullException>(
            () => BatchLookupOrchestrator.ExecuteAsync(coordinator, null!));
    }
}
