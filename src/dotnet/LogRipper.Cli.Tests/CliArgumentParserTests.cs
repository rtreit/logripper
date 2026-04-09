using LogRipper.Cli;

namespace LogRipper.Cli.Tests;

public class CliArgumentParserTests
{
    [Fact]
    public void Parse_reads_command_without_endpoint_override()
    {
        var arguments = CliArgumentParser.Parse(["status"]);

        Assert.Equal("status", arguments.Command);
        Assert.Equal(CliArgumentParser.DefaultEndpoint, arguments.Endpoint);
        Assert.False(arguments.ShowHelp);
    }

    [Fact]
    public void Parse_skips_endpoint_value_when_finding_command()
    {
        var arguments = CliArgumentParser.Parse(["--endpoint", "http://localhost:6001", "status"]);

        Assert.Equal("status", arguments.Command);
        Assert.Equal("http://localhost:6001", arguments.Endpoint);
        Assert.False(arguments.ShowHelp);
    }

    [Fact]
    public void Parse_returns_error_when_endpoint_value_is_missing()
    {
        var arguments = CliArgumentParser.Parse(["--endpoint"]);

        Assert.True(arguments.ShowHelp);
        Assert.Equal("Missing value for --endpoint.", arguments.Error);
    }
}
