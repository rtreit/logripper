using Microsoft.Extensions.Hosting;

namespace LogRipper.DebugHost.Services;

public sealed class RepositoryPaths
{
    public RepositoryPaths(IHostEnvironment hostEnvironment)
        : this(hostEnvironment.ContentRootPath)
    {
    }

    public RepositoryPaths(string contentRootPath)
    {
        ContentRoot = Path.GetFullPath(contentRootPath);
        RepoRoot = Path.GetFullPath(Path.Combine(ContentRoot, "..", "..", ".."));
        RustWorkspaceRoot = Path.Combine(RepoRoot, "src", "rust");
        DebugSolutionPath = Path.Combine(RepoRoot, "src", "dotnet", "LogRipper.Debug.sln");
    }

    public string ContentRoot { get; }

    public string RepoRoot { get; }

    public string RustWorkspaceRoot { get; }

    public string DebugSolutionPath { get; }
}
