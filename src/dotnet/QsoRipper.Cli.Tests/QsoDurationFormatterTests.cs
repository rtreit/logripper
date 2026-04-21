using QsoRipper.EngineSelection;

namespace QsoRipper.Cli.Tests;

public sealed class QsoDurationFormatterTests
{
    [Theory]
    [InlineData(0)]
    [InlineData(-1)]
    [InlineData(-3600)]
    public void FormatSecondsReturnsNullForNonPositiveValues(long seconds)
    {
        Assert.Null(QsoDurationFormatter.FormatSeconds(seconds));
    }

    [Theory]
    [InlineData(1, "1s")]
    [InlineData(45, "45s")]
    [InlineData(59, "59s")]
    public void FormatSecondsFormatsSecondsOnlyUnderOneMinute(long seconds, string expected)
    {
        Assert.Equal(expected, QsoDurationFormatter.FormatSeconds(seconds));
    }

    [Theory]
    [InlineData(60, "1m 00s")]
    [InlineData(155, "2m 35s")]
    [InlineData(3599, "59m 59s")]
    public void FormatSecondsFormatsMinutesAndSecondsUnderOneHour(long seconds, string expected)
    {
        Assert.Equal(expected, QsoDurationFormatter.FormatSeconds(seconds));
    }

    [Theory]
    [InlineData(3600, "1h 00m")]
    [InlineData(4320, "1h 12m")]
    [InlineData(7265, "2h 01m")]
    [InlineData(86400, "24h 00m")]
    public void FormatSecondsFormatsHoursAndMinutesAtOrAboveOneHour(long seconds, string expected)
    {
        Assert.Equal(expected, QsoDurationFormatter.FormatSeconds(seconds));
    }

    [Fact]
    public void FormatReturnsNullWhenEitherTimestampIsMissing()
    {
        var t = DateTimeOffset.UtcNow;
        Assert.Null(QsoDurationFormatter.Format(null, null));
        Assert.Null(QsoDurationFormatter.Format(t, null));
        Assert.Null(QsoDurationFormatter.Format(null, t));
    }

    [Fact]
    public void FormatReturnsNullWhenEndIsNotAfterStart()
    {
        var start = DateTimeOffset.UtcNow;
        Assert.Null(QsoDurationFormatter.Format(start, start));
        Assert.Null(QsoDurationFormatter.Format(start, start.AddSeconds(-30)));
    }

    [Fact]
    public void FormatUsesElapsedSeconds()
    {
        var start = new DateTimeOffset(2026, 4, 21, 0, 0, 0, TimeSpan.Zero);
        Assert.Equal("45s", QsoDurationFormatter.Format(start, start.AddSeconds(45)));
        Assert.Equal("2m 35s", QsoDurationFormatter.Format(start, start.AddSeconds(155)));
        Assert.Equal("1h 12m", QsoDurationFormatter.Format(start, start.AddSeconds(4320)));
    }

    [Fact]
    public void FormatTruncatesFractionalSeconds()
    {
        var start = new DateTimeOffset(2026, 4, 21, 0, 0, 0, TimeSpan.Zero);
        var end = start.AddMilliseconds(45_999);
        Assert.Equal("45s", QsoDurationFormatter.Format(start, end));
    }
}
