using LogRipper.Cli;

namespace LogRipper.Cli.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
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

    [Theory]
    [InlineData("http://localhost:50051", true)]
    [InlineData("https://example.com:7443", true)]
    [InlineData("localhost:50051", false)]
    [InlineData("grpc://localhost:50051", false)]
    [InlineData("not a uri", false)]
    public void Endpoint_validator_accepts_only_absolute_http_uris(string endpoint, bool expected)
    {
        var isValid = CliEndpointValidator.TryCreateEndpointUri(endpoint, out var uri);

        Assert.Equal(expected, isValid);

        if (expected)
        {
            Assert.NotNull(uri);
        }
    }
}
#pragma warning restore CA1707
