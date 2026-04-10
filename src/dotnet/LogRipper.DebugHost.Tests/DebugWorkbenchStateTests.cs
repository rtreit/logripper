using LogRipper.DebugHost.Models;
using LogRipper.DebugHost.Services;
using Microsoft.Extensions.Options;

namespace LogRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class DebugWorkbenchStateTests
{
    [Fact]
    public void Initializes_storage_defaults_from_options()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions
        {
            DefaultEngineEndpoint = "http://localhost:60051",
            DefaultEngineStorageBackend = "sqlite",
            DefaultEngineSqlitePath = @".\data\test-logripper.db"
        }));

        Assert.Equal("http://localhost:60051", state.EngineEndpoint);
        Assert.Equal(EngineStorageBackend.Sqlite, state.EngineStorageBackend);
        Assert.Equal(@".\data\test-logripper.db", state.EngineSqlitePath);
        Assert.Contains("--storage sqlite", state.BuildRustServerCommand(), StringComparison.Ordinal);
        Assert.Contains("--sqlite-path .\\data\\test-logripper.db", state.BuildRustServerCommand(), StringComparison.Ordinal);
    }

    [Fact]
    public void Update_storage_options_switches_back_to_memory()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateEngineEndpoint(" http://localhost:50061 ");
        state.UpdateStorageOptions(EngineStorageBackend.Memory, "   ");

        Assert.Equal("http://localhost:50061", state.EngineEndpoint);
        Assert.Equal(EngineStorageBackend.Memory, state.EngineStorageBackend);
        Assert.Equal("memory", state.GetEngineEnvironmentOverrides()["LOGRIPPER_STORAGE_BACKEND"]);
        Assert.DoesNotContain("LOGRIPPER_SQLITE_PATH", state.GetEngineEnvironmentOverrides().Keys, StringComparer.Ordinal);
        Assert.Equal("cargo run -p logripper-server -- --storage memory", state.BuildRustServerCommand());
    }
}
#pragma warning restore CA1707
