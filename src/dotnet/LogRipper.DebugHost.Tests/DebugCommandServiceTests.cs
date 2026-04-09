using LogRipper.DebugHost.Models;
using LogRipper.DebugHost.Services;
using Microsoft.Extensions.Options;

namespace LogRipper.DebugHost.Tests;

public class DebugCommandServiceTests
{
    [Fact]
    public void Command_catalog_includes_expected_debug_commands()
    {
        var service = new DebugCommandService(
            new RepositoryPaths(@"C:\repo\src\dotnet\LogRipper.DebugHost"),
            new ToolchainLocator(),
            Options.Create(new DebugWorkbenchOptions()));

        var commands = service.GetCommands();

        Assert.Contains(commands, static command => command.Key == "cargo-test" && command.RequiresProtoc);
        Assert.Contains(commands, static command => command.Key == "dotnet-test");
        Assert.Contains(commands, static command => command.Key == "buf-lint" && command.RequiredTool == "buf");
    }
}
