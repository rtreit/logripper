using LogRipper.Cli.Commands;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.Cli.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class CommandHelperTests
{
    [Fact]
    public void TryBuildQso_populates_expected_fields()
    {
        var success = LogQsoCommand.TryBuildQso(
            "W1AW",
            ["20m", "FT8", "--station", "k7abv", "--rst-sent", "59", "--rst-rcvd", "57", "--freq", "14074"],
            out var qso,
            out _,
            out var error);

        Assert.True(success);
        Assert.Null(error);
        Assert.NotNull(qso);
        Assert.Equal("W1AW", qso!.WorkedCallsign);
        Assert.Equal("K7ABV", qso.StationCallsign);
        Assert.Equal(Band._20M, qso.Band);
        Assert.Equal(Mode.Ft8, qso.Mode);
        Assert.Equal((ulong)14074, qso.FrequencyKhz);
        Assert.Equal((uint)5, qso.RstSent!.Readability);
        Assert.Equal((uint)9, qso.RstSent.Strength);
        Assert.Equal((uint)5, qso.RstReceived!.Readability);
        Assert.Equal((uint)7, qso.RstReceived.Strength);
        Assert.NotNull(qso.UtcTimestamp);
    }

    [Theory]
    [InlineData("--freq", "nope", "Invalid value for --freq: nope")]
    [InlineData("--rst-sent", "ab", "Invalid value for --rst-sent: ab. Expected 2 or 3 digits.")]
    [InlineData("--rst-rcvd", "1", "Invalid value for --rst-rcvd: 1. Expected 2 or 3 digits.")]
    public void TryBuildQso_rejects_invalid_optional_values(string option, string value, string expectedError)
    {
        var success = LogQsoCommand.TryBuildQso(
            "W1AW",
            ["20m", "FT8", option, value],
            out _,
            out _,
            out var error);

        Assert.False(success);
        Assert.Equal(expectedError, error);
    }

    [Theory]
    [InlineData("--station", "Missing value for --station.")]
    [InlineData("--freq", "Missing value for --freq.")]
    public void TryBuildQso_rejects_missing_option_values(string option, string expectedError)
    {
        var success = LogQsoCommand.TryBuildQso("W1AW", ["20m", "FT8", option], out _, out _, out var error);

        Assert.False(success);
        Assert.Equal(expectedError, error);
    }

    [Fact]
    public void TryParseRst_accepts_three_digits()
    {
        var success = LogQsoCommand.TryParseRst("599", out var report);

        Assert.True(success);
        Assert.Equal((uint)5, report.Readability);
        Assert.Equal((uint)9, report.Strength);
        Assert.Equal((uint)9, report.Tone);
    }

    [Theory]
    [InlineData("")]
    [InlineData("5")]
    [InlineData("abcd")]
    public void TryParseRst_rejects_invalid_values(string value)
    {
        Assert.False(LogQsoCommand.TryParseRst(value, out _));
    }

    [Theory]
    [InlineData(Mode.Ssb, 5u, 9u, 0u)]
    [InlineData(Mode.Am, 5u, 9u, 0u)]
    [InlineData(Mode.Fm, 5u, 9u, 0u)]
    [InlineData(Mode.Cw, 5u, 9u, 9u)]
    [InlineData(Mode.Ft8, 5u, 9u, 9u)]
    [InlineData(Mode.Rtty, 5u, 9u, 9u)]
    public void DefaultRst_returns_59_for_phone_and_599_for_digital(Mode mode, uint readability, uint strength, uint tone)
    {
        var rst = LogQsoCommand.DefaultRst(mode);

        Assert.Equal(readability, rst.Readability);
        Assert.Equal(strength, rst.Strength);
        Assert.Equal(tone, rst.Tone);
    }

    [Fact]
    public void ApplyDefaultRst_does_not_overwrite_explicit_values()
    {
        var qso = new QsoRecord
        {
            Mode = Mode.Cw,
            RstSent = new RstReport { Readability = 4, Strength = 7, Tone = 8 },
        };

        LogQsoCommand.ApplyDefaultRst(qso);

        Assert.Equal(4u, qso.RstSent.Readability);
        Assert.Equal(7u, qso.RstSent.Strength);
        Assert.Equal(8u, qso.RstSent.Tone);
        Assert.Equal(5u, qso.RstReceived!.Readability);
        Assert.Equal(9u, qso.RstReceived.Strength);
        Assert.Equal(9u, qso.RstReceived.Tone);
    }

    [Fact]
    public void TryBuildQso_without_rst_flags_gets_defaults_after_apply()
    {
        var success = LogQsoCommand.TryBuildQso("W1AW", ["20m", "SSB"], out var qso, out _, out _);

        Assert.True(success);
        Assert.Null(qso!.RstSent);

        LogQsoCommand.ApplyDefaultRst(qso);

        Assert.Equal(5u, qso.RstSent!.Readability);
        Assert.Equal(9u, qso.RstSent.Strength);
        Assert.Equal(0u, qso.RstSent.Tone);
        Assert.Equal(5u, qso.RstReceived!.Readability);
        Assert.Equal(9u, qso.RstReceived.Strength);
        Assert.Equal(0u, qso.RstReceived.Tone);
    }

    [Fact]
    public void TryParseArgs_populates_filters()
    {
        var success = ListQsosCommand.TryParseArgs(["--callsign", "w1aw", "--band", "20m", "--mode", "ft8", "--after", "2026-04-10T00:00:00Z", "--before", "2026-04-11T00:00:00Z", "--limit", "5"],
            out var request,
            out _,
            out var error);

        Assert.True(success);
        Assert.Null(error);
        Assert.Equal("W1AW", request.CallsignFilter);
        Assert.Equal(Band._20M, request.BandFilter);
        Assert.Equal(Mode.Ft8, request.ModeFilter);
        Assert.Equal((uint)5, request.Limit);
        Assert.NotNull(request.After);
        Assert.NotNull(request.Before);
    }

    public static TheoryData<string[], string> InvalidListArgs { get; } =
        new()
        {
            { ["--limit", "oops"], "Invalid value for --limit: oops" },
            { ["--after", "yesterdayish"], "Invalid --after value. Use relative (2.days, 3.hours) or absolute (2026-04-10)." },
            { ["--callsign"], "Missing value for --callsign." },
            { ["--unknown"], "Unknown option: --unknown" }
        };

    [Theory]
    [MemberData(nameof(InvalidListArgs))]
    public void TryParseArgs_rejects_invalid_args(string[] args, string expectedError)
    {
        var success = ListQsosCommand.TryParseArgs(args, out _, out _, out var error);

        Assert.False(success);
        Assert.Equal(expectedError, error);
    }

    public static TheoryData<RstReport?, string> FormattedRstValues { get; } =
        new()
        {
            { null, "" },
            { new RstReport { Readability = 5, Strength = 9 }, "59" },
            { new RstReport { Readability = 5, Strength = 9, Tone = 9 }, "599" }
        };

    [Theory]
    [MemberData(nameof(FormattedRstValues))]
    public void FormatRst_formats_reports(RstReport? rst, string expected)
    {
        Assert.Equal(expected, ListQsosCommand.FormatRst(rst));
    }

    [Fact]
    public void TryApplyUpdates_sets_grid_and_freq()
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW", Band = Band._20M, Mode = Mode.Cw };
        var enrich = false;

        var success = UpdateQsoCommand.TryApplyUpdates(
            ["--grid", "FN31", "--freq", "14035", "--comment", "Nice signal"],
            qso,
            ref enrich,
            out var error);

        Assert.True(success);
        Assert.Null(error);
        Assert.Equal("FN31", qso.WorkedGrid);
        Assert.Equal((ulong)14035, qso.FrequencyKhz);
        Assert.Equal("Nice signal", qso.Comment);
        Assert.False(enrich);
    }

    [Fact]
    public void TryApplyUpdates_sets_enrich_flag()
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW" };
        var enrich = false;

        var success = UpdateQsoCommand.TryApplyUpdates(["--enrich"], qso, ref enrich, out _);

        Assert.True(success);
        Assert.True(enrich);
    }

    [Fact]
    public void TryApplyUpdates_sets_country_state_band_mode()
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW", Band = Band._20M, Mode = Mode.Cw };
        var enrich = false;

        var success = UpdateQsoCommand.TryApplyUpdates(
            ["--country", "United States", "--state", "ct", "--band", "40m", "--mode", "SSB"],
            qso,
            ref enrich,
            out var error);

        Assert.True(success);
        Assert.Null(error);
        Assert.Equal("United States", qso.WorkedCountry);
        Assert.Equal("CT", qso.WorkedState);
        Assert.Equal(Band._40M, qso.Band);
        Assert.Equal(Mode.Ssb, qso.Mode);
    }

    [Fact]
    public void TryApplyUpdates_sets_rst_sent_and_received()
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW", Band = Band._20M, Mode = Mode.Cw };
        var enrich = false;

        var success = UpdateQsoCommand.TryApplyUpdates(
            ["--rst-sent", "579", "--rst-rcvd", "589"],
            qso,
            ref enrich,
            out var error);

        Assert.True(success);
        Assert.Null(error);
        Assert.Equal(5u, qso.RstSent!.Readability);
        Assert.Equal(7u, qso.RstSent.Strength);
        Assert.Equal(9u, qso.RstSent.Tone);
        Assert.Equal(5u, qso.RstReceived!.Readability);
        Assert.Equal(8u, qso.RstReceived.Strength);
        Assert.Equal(9u, qso.RstReceived.Tone);
    }

    [Theory]
    [InlineData("--grid", "Missing value for --grid.")]
    [InlineData("--country", "Missing value for --country.")]
    [InlineData("--state", "Missing value for --state.")]
    [InlineData("--freq", "Missing value for --freq.")]
    [InlineData("--band", "Missing value for --band.")]
    [InlineData("--mode", "Missing value for --mode.")]
    [InlineData("--comment", "Missing value for --comment.")]
    [InlineData("--rst-sent", "Missing value for --rst-sent.")]
    [InlineData("--rst-rcvd", "Missing value for --rst-rcvd.")]
    public void TryApplyUpdates_rejects_missing_values(string option, string expectedError)
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW" };
        var enrich = false;

        var success = UpdateQsoCommand.TryApplyUpdates([option], qso, ref enrich, out var error);

        Assert.False(success);
        Assert.Equal(expectedError, error);
    }

    [Fact]
    public void TryApplyUpdates_rejects_unknown_option()
    {
        var qso = new QsoRecord { WorkedCallsign = "W1AW" };
        var enrich = false;

        var success = UpdateQsoCommand.TryApplyUpdates(["--bogus"], qso, ref enrich, out var error);

        Assert.False(success);
        Assert.Equal("Unknown option: --bogus", error);
    }

    [Fact]
    public async Task ReadRequestsAsync_splits_stream_into_chunks()
    {
        var bytes = Enumerable.Range(0, 131077).Select(static i => (byte)(i % 251)).ToArray();
        using var stream = new MemoryStream(bytes);

        var requests = new List<ImportAdifRequest>();
        await foreach (var request in ImportAdifCommand.ReadRequestsAsync(stream))
        {
            requests.Add(request);
        }

        Assert.Equal(3, requests.Count);
        Assert.Equal(65536, requests[0].Chunk!.Data.Length);
        Assert.Equal(65536, requests[1].Chunk!.Data.Length);
        Assert.Equal(5, requests[2].Chunk!.Data.Length);

        var reconstructed = requests.SelectMany(static request => request.Chunk!.Data.ToByteArray()).ToArray();
        Assert.Equal(bytes, reconstructed);
    }
}
#pragma warning restore CA1707
