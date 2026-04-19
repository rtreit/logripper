namespace QsoRipper.Gui.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class AsyncVoidHandlerSafetyTests
{
    [Fact]
    public void MainWindowViewModel_async_void_handlers_have_top_level_exception_guards()
    {
        var source = File.ReadAllText(GetSourcePath("src", "dotnet", "QsoRipper.Gui", "ViewModels", "MainWindowViewModel.cs"));

        AssertMethodContains(source, "OnQsoLogged", "try");
        AssertMethodContains(source, "OnQsoLogged", "catch");

        AssertMethodContains(source, "OnRigTimerTick", "catch (Grpc.Core.RpcException)");
        AssertMethodContains(source, "OnRigTimerTick", "catch (ObjectDisposedException)");

        AssertMethodContains(source, "OnSpaceWeatherTimerTick", "try");
        AssertMethodContains(source, "OnSpaceWeatherTimerTick", "catch");
    }

    [Fact]
    public void MainWindow_async_void_handlers_have_top_level_exception_guards()
    {
        var source = File.ReadAllText(GetSourcePath("src", "dotnet", "QsoRipper.Gui", "Views", "MainWindow.axaml.cs"));

        AssertMethodContains(source, "OnOpened", "try");
        AssertMethodContains(source, "OnOpened", "catch");
        AssertMethodContains(source, "OnSettingsRequested", "try");
        AssertMethodContains(source, "OnSettingsRequested", "catch");
    }

    private static void AssertMethodContains(string source, string methodName, string expected)
    {
        var methodIndex = source.IndexOf($"{methodName}(", StringComparison.Ordinal);
        Assert.True(methodIndex >= 0, $"Method '{methodName}' was not found.");
        var snippet = source.Substring(methodIndex, Math.Min(3000, source.Length - methodIndex));
        Assert.Contains(expected, snippet, StringComparison.Ordinal);
    }

    private static string GetSourcePath(params string[] segments)
    {
        var directory = new DirectoryInfo(AppContext.BaseDirectory);
        while (directory is not null)
        {
            var candidate = Path.Combine([directory.FullName, .. segments]);
            if (File.Exists(candidate))
            {
                return candidate;
            }

            directory = directory.Parent;
        }

        throw new InvalidOperationException("Could not locate source file from test output directory.");
    }
}
#pragma warning restore CA1707
