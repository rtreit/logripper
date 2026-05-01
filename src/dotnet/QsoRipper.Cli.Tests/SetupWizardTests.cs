using QsoRipper.Cli.Commands;
using QsoRipper.EngineSelection;

namespace QsoRipper.Cli.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class SetupWizardTests
{
    [Fact]
    public void PromptField_returns_default_on_empty_input()
    {
        var originalIn = Console.In;
        var originalOut = Console.Out;
        try
        {
            Console.SetIn(new StringReader(Environment.NewLine));
            Console.SetOut(TextWriter.Null);
            var result = SetupCommand.PromptField("Test", "default_value");
            Assert.Equal("default_value", result);
        }
        finally
        {
            Console.SetIn(originalIn);
            Console.SetOut(originalOut);
        }
    }

    [Fact]
    public void PromptField_returns_user_input_when_provided()
    {
        var originalIn = Console.In;
        var originalOut = Console.Out;
        try
        {
            Console.SetIn(new StringReader("user_input" + Environment.NewLine));
            Console.SetOut(TextWriter.Null);
            var result = SetupCommand.PromptField("Test", "default_value");
            Assert.Equal("user_input", result);
        }
        finally
        {
            Console.SetIn(originalIn);
            Console.SetOut(originalOut);
        }
    }

    [Fact]
    public void PromptField_trims_whitespace()
    {
        var originalIn = Console.In;
        var originalOut = Console.Out;
        try
        {
            Console.SetIn(new StringReader("  trimmed  " + Environment.NewLine));
            Console.SetOut(TextWriter.Null);
            var result = SetupCommand.PromptField("Test", "default");
            Assert.Equal("trimmed", result);
        }
        finally
        {
            Console.SetIn(originalIn);
            Console.SetOut(originalOut);
        }
    }

    [Theory]
    [InlineData("y", true, true)]
    [InlineData("Y", true, true)]
    [InlineData("yes", true, true)]
    [InlineData("YES", true, true)]
    [InlineData("n", true, false)]
    [InlineData("N", true, false)]
    [InlineData("no", true, false)]
    [InlineData("anything", true, false)]
    [InlineData("", true, true)]    // default yes
    [InlineData("", false, false)]  // default no
    [InlineData("y", false, true)]
    [InlineData("n", false, false)]
    public void PromptYesNo_handles_inputs(string input, bool defaultYes, bool expected)
    {
        var originalIn = Console.In;
        var originalOut = Console.Out;
        try
        {
            Console.SetIn(new StringReader(input + Environment.NewLine));
            Console.SetOut(TextWriter.Null);
            var result = SetupCommand.PromptYesNo("Question?", defaultYes);
            Assert.Equal(expected, result);
        }
        finally
        {
            Console.SetIn(originalIn);
            Console.SetOut(originalOut);
        }
    }

    [Fact]
    public void CliArguments_defaults_setup_flags_to_false()
    {
        var args = new CliArguments("setup", "http://localhost:50051", EngineCatalog.DefaultProfile);

        Assert.False(args.SetupStatus);
        Assert.False(args.SetupFromEnv);
    }

    [Fact]
    public void CliArguments_can_set_setup_status()
    {
        var args = new CliArguments("setup", "http://localhost:50051", EngineCatalog.DefaultProfile, SetupStatus: true);

        Assert.True(args.SetupStatus);
        Assert.False(args.SetupFromEnv);
    }

    [Fact]
    public void CliArguments_can_set_setup_from_env()
    {
        var args = new CliArguments("setup", "http://localhost:50051", EngineCatalog.DefaultProfile, SetupFromEnv: true);

        Assert.False(args.SetupStatus);
        Assert.True(args.SetupFromEnv);
    }
}
#pragma warning restore CA1707
