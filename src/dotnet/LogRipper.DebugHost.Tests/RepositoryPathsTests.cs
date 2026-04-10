using LogRipper.DebugHost.Services;

namespace LogRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class RepositoryPathsTests
{
    [Fact]
    public void Derives_expected_repo_paths_from_content_root()
    {
        var paths = new RepositoryPaths(@"C:\repo\src\dotnet\LogRipper.DebugHost");

        Assert.Equal(@"C:\repo", paths.RepoRoot);
        Assert.Equal(Path.Combine(@"C:\repo", "src", "rust"), paths.RustWorkspaceRoot);
        Assert.Equal(Path.Combine(@"C:\repo", "src", "dotnet", "LogRipper.slnx"), paths.DotnetWorkspaceSolutionPath);
    }
}
#pragma warning restore CA1707
