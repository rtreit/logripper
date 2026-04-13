using Microsoft.Extensions.Hosting;

namespace QsoRipper.DebugHost.Services;

internal sealed class RepositoryPaths
{
    public RepositoryPaths(IHostEnvironment hostEnvironment)
        : this(hostEnvironment.ContentRootPath)
    {
    }

    public RepositoryPaths(string contentRootPath)
    {
        ArgumentNullException.ThrowIfNull(contentRootPath);

        ContentRoot = Path.GetFullPath(contentRootPath);
        RepoRoot = Path.GetFullPath(Path.Combine(ContentRoot, "..", "..", ".."));
        RustWorkspaceRoot = Path.Combine(RepoRoot, "src", "rust");
        DotnetWorkspaceSolutionPath = Path.Combine(RepoRoot, "src", "dotnet", "QsoRipper.slnx");
    }

    public string ContentRoot { get; }

    public string RepoRoot { get; }

    public string RustWorkspaceRoot { get; }

    public string DotnetWorkspaceSolutionPath { get; }
}
