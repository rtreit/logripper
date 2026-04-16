using QsoRipper.Domain;
using QsoRipper.Engine.Lookup.Qrz;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class LookupCoordinatorTests
{
    private sealed class FakeProvider : ICallsignProvider
    {
        private readonly Queue<ProviderLookupResult> _responses = new();
        private int _callCount;

        public string ProviderName => "fake";
        public int CallCount => _callCount;

        public void Enqueue(ProviderLookupResult result) => _responses.Enqueue(result);

        public Task<ProviderLookupResult> LookupAsync(string callsign, CancellationToken ct = default)
        {
            Interlocked.Increment(ref _callCount);
            return Task.FromResult(_responses.Count > 0
                ? _responses.Dequeue()
                : new ProviderLookupResult { State = ProviderLookupState.NotFound });
        }
    }

    private static ProviderLookupResult FoundResult(string callsign, string firstName = "Test") =>
        new()
        {
            State = ProviderLookupState.Found,
            Record = new CallsignRecord
            {
                Callsign = callsign,
                CrossRef = callsign,
                FirstName = firstName,
                LastName = "Operator",
            },
        };

    [Fact]
    public async Task Lookup_ReturnsFoundResult()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW"));
        var coordinator = new LookupCoordinator(provider);

        var result = await coordinator.LookupAsync("W1AW");

        Assert.Equal(LookupState.Found, result.State);
        Assert.False(result.CacheHit);
        Assert.Equal("W1AW", result.QueriedCallsign);
        Assert.NotNull(result.Record);
        Assert.Equal("W1AW", result.Record.Callsign);
    }

    [Fact]
    public async Task Lookup_ReturnsCacheHitOnSecondCall()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW"));
        var coordinator = new LookupCoordinator(provider);

        var first = await coordinator.LookupAsync("w1aw");
        var second = await coordinator.LookupAsync("w1aw");

        Assert.Equal(LookupState.Found, first.State);
        Assert.False(first.CacheHit);
        Assert.Equal(LookupState.Found, second.State);
        Assert.True(second.CacheHit);
        Assert.Equal(1, provider.CallCount);
    }

    [Fact]
    public async Task Lookup_SkipCacheForcesProviderLookup()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW", "First"));
        provider.Enqueue(FoundResult("W1AW", "Second"));
        var coordinator = new LookupCoordinator(provider);

        _ = await coordinator.LookupAsync("w1aw");
        var second = await coordinator.LookupAsync("w1aw", skipCache: true);

        Assert.Equal(LookupState.Found, second.State);
        Assert.False(second.CacheHit);
        Assert.Equal(2, provider.CallCount);
    }

    [Fact]
    public async Task Lookup_NormalizesToUppercase()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW"));
        var coordinator = new LookupCoordinator(provider);

        var result = await coordinator.LookupAsync("  w1aw  ");

        Assert.Equal("W1AW", result.QueriedCallsign);
        Assert.Equal(LookupState.Found, result.State);
    }

    [Fact]
    public async Task Lookup_NotFound_IsCachedWithNegativeTtl()
    {
        var provider = new FakeProvider();
        provider.Enqueue(new ProviderLookupResult { State = ProviderLookupState.NotFound });
        var coordinator = new LookupCoordinator(provider, negativeTtl: TimeSpan.FromMinutes(5));

        var first = await coordinator.LookupAsync("NOTEXIST");
        var second = await coordinator.LookupAsync("NOTEXIST");

        Assert.Equal(LookupState.NotFound, first.State);
        Assert.Equal(LookupState.NotFound, second.State);
        Assert.True(second.CacheHit);
        Assert.Equal(1, provider.CallCount);
    }

    [Fact]
    public async Task Lookup_SlashCallFallsBackToBase()
    {
        var provider = new FakeProvider();
        // First lookup for K7ABC/M returns not found, second for K7ABC returns found
        provider.Enqueue(new ProviderLookupResult { State = ProviderLookupState.NotFound });
        provider.Enqueue(FoundResult("K7ABC"));
        var coordinator = new LookupCoordinator(provider);

        var result = await coordinator.LookupAsync("K7ABC/M");

        Assert.Equal(LookupState.Found, result.State);
        Assert.Equal("K7ABC/M", result.QueriedCallsign);
        Assert.Equal(2, provider.CallCount);
    }

    [Fact]
    public async Task Lookup_NoSlashNoFallback()
    {
        var provider = new FakeProvider();
        provider.Enqueue(new ProviderLookupResult { State = ProviderLookupState.NotFound });
        var coordinator = new LookupCoordinator(provider);

        var result = await coordinator.LookupAsync("W1AW");

        Assert.Equal(LookupState.NotFound, result.State);
        Assert.Equal(1, provider.CallCount);
    }

    [Fact]
    public async Task Lookup_SlashCallExactFoundDoesNotFallback()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("K7ABC/M"));
        var coordinator = new LookupCoordinator(provider);

        var result = await coordinator.LookupAsync("K7ABC/M");

        Assert.Equal(LookupState.Found, result.State);
        Assert.Equal(1, provider.CallCount);
    }

    [Fact]
    public async Task GetCached_ReturnsCachedResult()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW"));
        var coordinator = new LookupCoordinator(provider);

        _ = await coordinator.LookupAsync("W1AW");
        var cached = await coordinator.GetCachedAsync("W1AW");

        Assert.Equal(LookupState.Found, cached.State);
    }

    [Fact]
    public async Task GetCached_ReturnsNotFoundWhenNotCached()
    {
        var provider = new FakeProvider();
        var coordinator = new LookupCoordinator(provider);

        var cached = await coordinator.GetCachedAsync("W1AW");

        Assert.Equal(LookupState.NotFound, cached.State);
        Assert.False(cached.CacheHit);
    }

    [Fact]
    public async Task StreamLookup_EmitsLoadingThenFound()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW"));
        var coordinator = new LookupCoordinator(provider);

        var updates = await coordinator.StreamLookupAsync("W1AW");

        Assert.Equal(2, updates.Length);
        Assert.Equal(LookupState.Loading, updates[0].State);
        Assert.Equal(LookupState.Found, updates[1].State);
    }

    [Fact]
    public async Task StreamLookup_EmitsLoadingStaleThenRefreshed()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW", "Cached"));
        provider.Enqueue(FoundResult("W1AW", "Fresh"));
        // Use 1ms positive TTL so the cache entry goes stale immediately.
        var coordinator = new LookupCoordinator(provider, positiveTtl: TimeSpan.FromMilliseconds(1));

        _ = await coordinator.LookupAsync("W1AW");
        await Task.Delay(5);
        var updates = await coordinator.StreamLookupAsync("W1AW");

        Assert.Equal(3, updates.Length);
        Assert.Equal(LookupState.Loading, updates[0].State);
        Assert.Equal(LookupState.Stale, updates[1].State);
        Assert.Equal("Cached", updates[1].Record!.FirstName);
        Assert.Equal(LookupState.Found, updates[2].State);
        Assert.Equal("Fresh", updates[2].Record!.FirstName);
    }

    [Fact]
    public async Task StreamLookup_FreshCacheReturnsCacheHit()
    {
        var provider = new FakeProvider();
        provider.Enqueue(FoundResult("W1AW"));
        var coordinator = new LookupCoordinator(provider, positiveTtl: TimeSpan.FromMinutes(15));

        _ = await coordinator.LookupAsync("W1AW");
        var updates = await coordinator.StreamLookupAsync("W1AW");

        Assert.Equal(2, updates.Length);
        Assert.Equal(LookupState.Loading, updates[0].State);
        Assert.Equal(LookupState.Found, updates[1].State);
        Assert.True(updates[1].CacheHit);
        Assert.Equal(1, provider.CallCount);
    }

    [Fact]
    public async Task Lookup_ProviderError_ReturnsErrorState()
    {
        var provider = new FakeProvider();
        provider.Enqueue(new ProviderLookupResult
        {
            State = ProviderLookupState.AuthenticationError,
            ErrorMessage = "Bad credentials",
        });
        var coordinator = new LookupCoordinator(provider);

        var result = await coordinator.LookupAsync("W1AW");

        Assert.Equal(LookupState.Error, result.State);
        Assert.Contains("Bad credentials", result.ErrorMessage, StringComparison.Ordinal);
    }

    [Fact]
    public async Task DisabledProvider_ReturnsNotFound()
    {
        var provider = new DisabledCallsignProvider();
        var result = await provider.LookupAsync("W1AW");

        Assert.Equal(ProviderLookupState.NotFound, result.State);
        Assert.Equal("disabled", provider.ProviderName);
    }
}
