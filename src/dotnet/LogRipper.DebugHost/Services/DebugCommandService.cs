using System.Diagnostics;
using LogRipper.DebugHost.Models;
using Microsoft.Extensions.Options;

namespace LogRipper.DebugHost.Services;

public sealed class DebugCommandService
{
    private readonly RepositoryPaths _repositoryPaths;
    private readonly ToolchainLocator _toolchainLocator;
    private readonly DebugWorkbenchOptions _options;

    public DebugCommandService(
        RepositoryPaths repositoryPaths,
        ToolchainLocator toolchainLocator,
        IOptions<DebugWorkbenchOptions> options)
    {
        _repositoryPaths = repositoryPaths;
        _toolchainLocator = toolchainLocator;
        _options = options.Value;
    }

    public IReadOnlyList<DebugCommandDefinition> GetCommands()
    {
        return
        [
            new(
                "cargo-test",
                "Run Rust workspace tests",
                "Runs the Rust workspace tests using the repo's current source tree.",
                "cargo",
                "test --manifest-path src\\rust\\Cargo.toml",
                _repositoryPaths.RepoRoot,
                RequiresProtoc: true),
            new(
                "cargo-adif-tests",
                "Run ADIF integration tests",
                "Runs only the existing ADIF integration tests from logripper-core.",
                "cargo",
                "test --manifest-path src\\rust\\Cargo.toml --test adif_integration",
                _repositoryPaths.RepoRoot,
                RequiresProtoc: true),
            new(
                "dotnet-test",
                "Run debug host tests",
                "Runs the .NET debug workbench test project through the local debug solution.",
                "dotnet",
                "test src\\dotnet\\LogRipper.Debug.sln",
                _repositoryPaths.RepoRoot),
            new(
                "buf-lint",
                "Run buf lint",
                "Runs the repository's protobuf linting command if buf is available on the machine.",
                "buf",
                "lint",
                _repositoryPaths.RepoRoot,
                RequiredTool: "buf")
        ];
    }

    public async Task<CommandExecutionResult> RunAsync(string key, CancellationToken cancellationToken = default)
    {
        var command = GetCommands().Single(command => command.Key == key);
        var effectiveEnvironment = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

        if (string.Equals(command.RequiredTool, "buf", StringComparison.OrdinalIgnoreCase) && _toolchainLocator.FindBuf() is null)
        {
            return CreateUnavailableResult(command, "The 'buf' executable is not available on PATH.");
        }

        if (command.RequiresProtoc)
        {
            var protocPath = _toolchainLocator.FindProtoc();
            if (string.IsNullOrWhiteSpace(protocPath))
            {
                return CreateUnavailableResult(command, "A protoc executable was not found. Install Protocol Buffers or restore Grpc.Tools.");
            }

            effectiveEnvironment["PROTOC"] = protocPath;

            var includeDirectory = _toolchainLocator.FindGrpcToolsIncludeDirectory();
            if (!string.IsNullOrWhiteSpace(includeDirectory))
            {
                effectiveEnvironment["PROTOC_INCLUDE"] = includeDirectory;
            }
        }

        var startedAt = DateTimeOffset.UtcNow;
        using var process = new Process
        {
            StartInfo = new ProcessStartInfo
            {
                FileName = command.FileName,
                Arguments = command.Arguments,
                WorkingDirectory = command.WorkingDirectory,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
                UseShellExecute = false
            }
        };

        foreach (var (name, value) in effectiveEnvironment)
        {
            process.StartInfo.Environment[name] = value;
        }

        process.Start();
        var standardOutput = process.StandardOutput.ReadToEndAsync(cancellationToken);
        var standardError = process.StandardError.ReadToEndAsync(cancellationToken);

        using var timeoutSource = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeoutSource.CancelAfter(TimeSpan.FromSeconds(_options.CommandTimeoutSeconds));
        await process.WaitForExitAsync(timeoutSource.Token);

        return new CommandExecutionResult(
            command,
            process.ExitCode,
            await standardOutput,
            await standardError,
            startedAt,
            DateTimeOffset.UtcNow,
            effectiveEnvironment);
    }

    private static CommandExecutionResult CreateUnavailableResult(DebugCommandDefinition command, string error)
    {
        var now = DateTimeOffset.UtcNow;
        return new CommandExecutionResult(command, -1, string.Empty, error, now, now, new Dictionary<string, string>());
    }
}
