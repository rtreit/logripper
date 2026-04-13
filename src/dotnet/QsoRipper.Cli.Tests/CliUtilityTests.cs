using System.Text;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class CliUtilityTests
{
    [Theory]
    [InlineData("lookup", true)]
    [InlineData("log", true)]
    [InlineData("import", true)]
    [InlineData("status", false)]
    [InlineData("list", false)]
    public void UsesPrimaryArgument_matches_command_metadata(string command, bool expected)
    {
        Assert.Equal(expected, CliCommandMetadata.UsesPrimaryArgument(command));
        Assert.Equal(expected, CliCommandMetadata.RequiresPrimaryArgument(command));
    }

    [Fact]
    public void IsCommandHelp_detects_help_tokens_in_primary_or_remaining_args()
    {
        var byPrimaryArg = new CliArguments("lookup", CliArgumentParser.DefaultEndpoint, Callsign: "-?");
        var byRemainingArg = new CliArguments("log", CliArgumentParser.DefaultEndpoint, RemainingArgs: ["--help"]);

        Assert.True(CliCommandMetadata.IsCommandHelp(byPrimaryArg));
        Assert.True(CliCommandMetadata.IsCommandHelp(byRemainingArg));
    }

    [Fact]
    public void GetGeneralHelp_includes_main_sections()
    {
        var help = CliHelpText.GetGeneralHelp();

        Assert.Contains("Logbook:", help, StringComparison.Ordinal);
        Assert.Contains("ADIF:", help, StringComparison.Ordinal);
        Assert.Contains("Lookup:", help, StringComparison.Ordinal);
        Assert.Contains("Engine:", help, StringComparison.Ordinal);
    }

    [Fact]
    public void GetCommandHelp_returns_specific_usage()
    {
        var help = CliHelpText.GetCommandHelp("import");

        Assert.Contains("Usage: import <file-path>", help, StringComparison.Ordinal);
        Assert.DoesNotContain("Usage: qsoripper-cli [options] <command> [arguments]", help, StringComparison.Ordinal);
    }

    public static TheoryData<string, string> CommandHelpCases { get; } =
        new()
        {
            { "status", "Usage: status" },
            { "config", "Usage: config [options]" },
            { "lookup", "Usage: lookup <callsign> [--skip-cache]" }
        };

    [Theory]
    [MemberData(nameof(CommandHelpCases))]
    public void GetCommandHelp_returns_expected_command_usage(string command, string expectedUsage)
    {
        var help = CliHelpText.GetCommandHelp(command);

        Assert.Contains(expectedUsage, help, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("20m", Band._20M)]
    [InlineData("2190m", Band._2190M)]
    [InlineData("160m", Band._160M)]
    [InlineData("6m", Band._6M)]
    [InlineData("70cm", Band._70Cm)]
    [InlineData("1.25cm", Band._125Cm)]
    [InlineData("2.5mm", Band._25Mm)]
    [InlineData("submm", Band.Submm)]
    public void ParseBand_accepts_supported_inputs(string input, Band expected)
    {
        Assert.Equal(expected, EnumHelpers.ParseBand(input));
    }

    [Theory]
    [InlineData(Band._2190M, "2190M")]
    [InlineData(Band._160M, "160M")]
    [InlineData(Band._20M, "20M")]
    [InlineData(Band._70Cm, "70CM")]
    [InlineData(Band._125M, "1.25M")]
    [InlineData(Band._125Cm, "1.25CM")]
    [InlineData(Band._25Mm, "2.5MM")]
    [InlineData(Band.Submm, "SUBMM")]
    public void FormatBand_returns_canonical_values(Band band, string expected)
    {
        Assert.Equal(expected, EnumHelpers.FormatBand(band));
    }

    [Theory]
    [InlineData("am", Mode.Am)]
    [InlineData("ft8", Mode.Ft8)]
    [InlineData("cw", Mode.Cw)]
    [InlineData("fm", Mode.Fm)]
    [InlineData("rtty", Mode.Rtty)]
    [InlineData("digitalvoice", Mode.Digitalvoice)]
    public void ParseMode_accepts_supported_inputs(string input, Mode expected)
    {
        Assert.Equal(expected, EnumHelpers.ParseMode(input));
    }

    [Theory]
    [InlineData(Mode.Am, "AM")]
    [InlineData(Mode.Ft8, "FT8")]
    [InlineData(Mode.Fm, "FM")]
    [InlineData(Mode.Rtty, "RTTY")]
    [InlineData(Mode.Ssb, "SSB")]
    public void FormatMode_returns_uppercase_name(Mode mode, string expected)
    {
        Assert.Equal(expected, EnumHelpers.FormatMode(mode));
    }

    [Theory]
    [InlineData("garbage")]
    [InlineData("1.lightyears")]
    public void TimeParser_rejects_invalid_inputs(string input)
    {
        Assert.Null(TimeParser.Parse(input));
    }

    [Fact]
    public void TimeParser_parses_relative_time()
    {
        var before = DateTime.UtcNow.AddMinutes(-3);
        var parsed = TimeParser.Parse("2.minutes");
        var after = DateTime.UtcNow.AddMinutes(-1);

        Assert.NotNull(parsed);
        Assert.InRange(parsed!.ToDateTime(), before, after);
    }

    [Fact]
    public void TimeParser_parses_absolute_time()
    {
        var parsed = TimeParser.Parse("2026-04-10T12:34:56Z");

        Assert.NotNull(parsed);
        Assert.Equal(new DateTime(2026, 4, 10, 12, 34, 56, DateTimeKind.Utc), parsed!.ToDateTime());
    }

    [Fact]
    public void JsonOutput_Print_writes_indented_json()
    {
        var output = CaptureConsoleOut(() => JsonOutput.Print(new GetSyncStatusResponse { LocalQsoCount = 3 }));

        Assert.Contains("\"localQsoCount\": 3", output, StringComparison.Ordinal);
    }

    [Fact]
    public void JsonOutput_PrintArray_writes_json_array()
    {
        var messages = new Google.Protobuf.IMessage[]
        {
            new GetSyncStatusResponse { LocalQsoCount = 1 },
            new GetSyncStatusResponse { LocalQsoCount = 2 }
        };

        var output = CaptureConsoleOut(() => JsonOutput.PrintArray(messages));

        Assert.StartsWith("[", output, StringComparison.Ordinal);
        Assert.Contains("\"localQsoCount\": 1", output, StringComparison.Ordinal);
        Assert.Contains("\"localQsoCount\": 2", output, StringComparison.Ordinal);
        Assert.EndsWith(Environment.NewLine + "]" + Environment.NewLine, output, StringComparison.Ordinal);
    }

    private static string CaptureConsoleOut(Action action)
    {
        var builder = new StringBuilder();
        using var writer = new StringWriter(builder);
        var original = Console.Out;

        try
        {
            Console.SetOut(writer);
            action();
        }
        finally
        {
            Console.SetOut(original);
        }

        return builder.ToString();
    }
}
#pragma warning restore CA1707
