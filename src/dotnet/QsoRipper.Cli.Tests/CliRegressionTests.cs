using QsoRipper.Domain;

namespace QsoRipper.Cli.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class CliRegressionTests
{
    [Fact]
    public async Task Entry_point_shows_command_help_for_log_help()
    {
        var result = await CliProcessRunner.RunAsync("log", "--help");

        Assert.Equal(0, result.ExitCode);
        Assert.Contains("Usage: log <callsign> <band> <mode> [options]", result.StandardOutput, StringComparison.Ordinal);
        Assert.DoesNotContain("Usage: qsoripper-cli [options] <command> [arguments]", result.StandardOutput, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Entry_point_shows_import_help_when_file_path_is_missing()
    {
        var result = await CliProcessRunner.RunAsync("import");

        Assert.Equal(0, result.ExitCode);
        Assert.Contains("Usage: import <file-path>", result.StandardOutput, StringComparison.Ordinal);
        Assert.DoesNotContain("File not found:", result.StandardError + result.StandardOutput, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Entry_point_rejects_invalid_frequency_before_connecting()
    {
        var result = await CliProcessRunner.RunAsync("log", "W1AW", "20m", "FT8", "--freq", "nope");

        Assert.Equal(1, result.ExitCode);
        Assert.Contains("Invalid value for --freq: nope", result.StandardError, StringComparison.Ordinal);
        Assert.DoesNotContain("Could not connect to QsoRipper engine", result.StandardError, StringComparison.Ordinal);
    }

    [Fact]
    public async Task Entry_point_rejects_invalid_rst_before_connecting()
    {
        var result = await CliProcessRunner.RunAsync("log", "W1AW", "20m", "FT8", "--rst-sent", "ab");

        Assert.Equal(1, result.ExitCode);
        Assert.Contains("Invalid value for --rst-sent: ab", result.StandardError, StringComparison.Ordinal);
        Assert.DoesNotContain("Could not connect to QsoRipper engine", result.StandardError, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData(Band._125M, "1.25M")]
    [InlineData(Band._125Cm, "1.25CM")]
    [InlineData(Band._25Mm, "2.5MM")]
    public void FormatBand_preserves_decimal_band_names(Band band, string expected)
    {
        Assert.Equal(expected, EnumHelpers.FormatBand(band));
    }

    [Fact]
    public void Parse_treats_help_after_command_as_command_specific_help()
    {
        var arguments = CliArgumentParser.Parse(["log", "--help"]);

        Assert.Equal("log", arguments.Command);
        Assert.False(arguments.ShowHelp);
        Assert.Equal(["--help"], arguments.RemainingArgs);
    }
}
#pragma warning restore CA1707
