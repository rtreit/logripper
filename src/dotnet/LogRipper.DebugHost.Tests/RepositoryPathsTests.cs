using LogRipper.DebugHost.Services;

namespace LogRipper.DebugHost.Tests;

public class RepositoryPathsTests
{
    [Fact]
    public void Derives_expected_repo_paths_from_content_root()
    {
        var paths = new RepositoryPaths(@"C:\repo\src\dotnet\LogRipper.DebugHost");

        Assert.Equal(@"C:\repo", paths.RepoRoot);
        Assert.Equal(Path.Combine(@"C:\repo", "src", "rust"), paths.RustWorkspaceRoot);
        Assert.Equal(Path.Combine(@"C:\repo", "src", "dotnet", "LogRipper.Debug.sln"), paths.DebugSolutionPath);
    }
}
