using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.Tests;

public sealed class CwDecoderProcessSampleSourceTests
{
    [Theory]
    [InlineData("{\"type\":\"char\",\"ch\":\"C\"}")]
    [InlineData("{\"type\":\"word\"}")]
    public void RollingBackendDecodedEventsPromoteToLocked(string line)
    {
        Assert.True(CwDecoderProcessSampleSource.TryParseRollingBackendState(line, out var state));
        Assert.Equal(CwLockState.Locked, state);
    }

    [Theory]
    [InlineData("{\"type\":\"power\",\"signal\":true}")]
    [InlineData("{\"type\":\"stats\",\"wpm\":20.0}")]
    [InlineData("{\"type\":\"pitch\",\"hz\":700.0}")]
    [InlineData("{\"type\":\"wpm\",\"wpm\":20.0}")]
    public void RollingBackendMeterEventsDoNotDemoteLockState(string line)
    {
        Assert.False(CwDecoderProcessSampleSource.TryParseRollingBackendState(line, out _));
    }

    [Fact]
    public void RollingBackendReadyInitializesHuntingState()
    {
        Assert.True(CwDecoderProcessSampleSource.TryParseRollingBackendState(
            "{\"type\":\"ready\",\"source\":\"stream-live-ditdah\"}",
            out var state));
        Assert.Equal(CwLockState.Hunting, state);
    }
}
