using System.Diagnostics;
using System.Linq;
using Microsoft.Extensions.Options;
using QsoRipper.DebugHost.Models;

namespace QsoRipper.DebugHost.Services;

internal sealed class DebugCommandService
{
    private readonly RepositoryPaths _repositoryPaths;
    private readonly ToolchainLocator _toolchainLocator;
    private readonly DebugWorkbenchOptions _options;

    public DebugCommandService(
        RepositoryPaths repositoryPaths,
        ToolchainLocator toolchainLocator,
        IOptions<DebugWorkbenchOptions> options)
    {
        ArgumentNullException.ThrowIfNull(repositoryPaths);
        ArgumentNullException.ThrowIfNull(toolchainLocator);
        ArgumentNullException.ThrowIfNull(options);

        _repositoryPaths = repositoryPaths;
        _toolchainLocator = toolchainLocator;
        _options = options.Value;
    }

    public IReadOnlyList<DebugCommandDefinition> GetCommands()
    {
        var rustManifestPath = _repositoryPaths.GetRepoRelativePath(_repositoryPaths.RustWorkspaceManifestPath);
        var dotnetSolutionPath = _repositoryPaths.GetRepoRelativePath(_repositoryPaths.DotnetWorkspaceSolutionPath);
        var conformanceScriptPath = _repositoryPaths.GetRepoRelativePath(_repositoryPaths.EngineConformanceScriptPath);

        return
        [
            new(
                "cargo-test",
                "Run Rust workspace tests",
                "Runs the Rust workspace tests using the repo's current source tree.",
                "cargo",
                BuildArguments("test", "--manifest-path", rustManifestPath),
                _repositoryPaths.RepoRoot,
                RequiresProtoc: true),
            new(
                "cargo-adif-tests",
                "Run ADIF integration tests",
                "Runs only the existing ADIF integration tests from qsoripper-core.",
                "cargo",
                BuildArguments("test", "--manifest-path", rustManifestPath, "--test", "adif_integration"),
                _repositoryPaths.RepoRoot,
                RequiresProtoc: true),
            new(
                "cargo-storage-tests",
                "Run storage adapter tests",
                "Runs the memory and SQLite storage adapter tests plus backend-selection coverage in the Rust server host.",
                "cargo",
                BuildArguments(
                    "test",
                    "--manifest-path",
                    rustManifestPath,
                    "-p",
                    "qsoripper-storage-memory",
                    "-p",
                    "qsoripper-storage-sqlite",
                    "-p",
                    "qsoripper-server"),
                _repositoryPaths.RepoRoot,
                RequiresProtoc: true),
            new(
                "dotnet-test",
                "Run .NET workspace tests",
                $"Runs the full .NET workspace test suite through the root {dotnetSolutionPath} solution.",
                "dotnet",
                BuildArguments("test", dotnetSolutionPath),
                _repositoryPaths.RepoRoot),
            new(
                "engine-conformance",
                "Run engine conformance scenario",
                "Starts the Rust and managed .NET engines one at a time and drives the same CLI setup/log/export scenario against both to verify client-visible parity.",
                "pwsh",
                BuildArguments("-File", conformanceScriptPath),
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
        ArgumentNullException.ThrowIfNull(key);

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
        try
        {
            await process.WaitForExitAsync(timeoutSource.Token);
        }
        catch (OperationCanceledException) when (!cancellationToken.IsCancellationRequested)
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }

            await process.WaitForExitAsync(cancellationToken);
            return CreateTimeoutResult(command, _options.CommandTimeoutSeconds, await standardOutput, await standardError, startedAt, effectiveEnvironment);
        }

        return new CommandExecutionResult(
            command,
            process.ExitCode,
            await standardOutput,
            await standardError,
            startedAt,
            DateTimeOffset.UtcNow,
            effectiveEnvironment);
    }

    private static CommandExecutionResult CreateTimeoutResult(
        DebugCommandDefinition command,
        int timeoutSeconds,
        string standardOutput,
        string standardError,
        DateTimeOffset startedAt,
        IReadOnlyDictionary<string, string> effectiveEnvironment)
    {
        var timeoutMessage = $"Command timed out after {timeoutSeconds} second(s).";
        var combinedError = string.IsNullOrWhiteSpace(standardError)
            ? timeoutMessage
            : $"{standardError}{Environment.NewLine}{timeoutMessage}";

        return new CommandExecutionResult(
            command,
            -1,
            standardOutput,
            combinedError,
            startedAt,
            DateTimeOffset.UtcNow,
            effectiveEnvironment);
    }

    private static CommandExecutionResult CreateUnavailableResult(DebugCommandDefinition command, string error)
    {
        var now = DateTimeOffset.UtcNow;
        return new CommandExecutionResult(command, -1, string.Empty, error, now, now, new Dictionary<string, string>());
    }

    private static string BuildArguments(params string[] arguments)
    {
        return string.Join(
            " ",
            arguments
                .Where(argument => !string.IsNullOrWhiteSpace(argument))
                .Select(QuoteArgument));
    }

    private static string QuoteArgument(string argument)
    {
        var escaped = argument.Replace("\"", "\\\"", StringComparison.Ordinal);
        return escaped.IndexOfAny([' ', '\t']) >= 0
            ? $"\"{escaped}\""
            : escaped;
    }
}
