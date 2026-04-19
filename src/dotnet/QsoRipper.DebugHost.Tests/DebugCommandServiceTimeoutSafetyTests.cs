namespace QsoRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class DebugCommandServiceTimeoutSafetyTests
{
    [Fact]
    public void RunAsync_catches_timeout_cancellation_and_kills_process_tree()
    {
        var source = File.ReadAllText(GetDebugCommandServicePath());

        Assert.Contains("catch (OperationCanceledException)", source, StringComparison.Ordinal);
        Assert.Contains("process.Kill(entireProcessTree: true)", source, StringComparison.Ordinal);
    }

    private static string GetDebugCommandServicePath()
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            var candidate = Path.Combine(directory.FullName, "src", "dotnet", "QsoRipper.DebugHost", "Services", "DebugCommandService.cs");
            if (File.Exists(candidate))
            {
                return candidate;
            }

            directory = directory.Parent;
        }

        throw new InvalidOperationException("Could not locate DebugCommandService.cs from test output directory.");
    }
}
#pragma warning restore CA1707
