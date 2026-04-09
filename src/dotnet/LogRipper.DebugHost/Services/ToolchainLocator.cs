using LogRipper.DebugHost.Models;

namespace LogRipper.DebugHost.Services;

public sealed class ToolchainLocator
{
    public string? FindCargo() => FindOnPath("cargo");

    public string? FindDotnet() => FindOnPath("dotnet");

    public string? FindBuf() => FindOnPath("buf");

    public string? FindSystemProtoc() => FindOnPath("protoc");

    public string? FindGrpcToolsProtoc()
    {
        var packageDirectory = FindLatestGrpcToolsDirectory();
        if (packageDirectory is null)
        {
            return null;
        }

        var executableName = OperatingSystem.IsWindows() ? "protoc.exe" : "protoc";
        var ridFolder = OperatingSystem.IsWindows() ? "windows_x64" : "linux_x64";
        var candidate = Path.Combine(packageDirectory, "tools", ridFolder, executableName);
        return File.Exists(candidate) ? candidate : null;
    }

    public string? FindProtoc() => FindSystemProtoc() ?? FindGrpcToolsProtoc();

    public string? FindGrpcToolsIncludeDirectory()
    {
        var packageDirectory = FindLatestGrpcToolsDirectory();
        if (packageDirectory is null)
        {
            return null;
        }

        var candidate = Path.Combine(packageDirectory, "build", "native", "include");
        return Directory.Exists(candidate) ? candidate : null;
    }

    public IReadOnlyList<ToolAvailability> GetToolAvailability()
    {
        return
        [
            new("dotnet", FindDotnet() is not null, FindDotnet()),
            new("cargo", FindCargo() is not null, FindCargo()),
            new("buf", FindBuf() is not null, FindBuf()),
            new("protoc", FindProtoc() is not null, FindProtoc())
        ];
    }

    private static IEnumerable<string> EnumerateNuGetPackageRoots()
    {
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        var candidates = new[]
        {
            Environment.GetEnvironmentVariable("NUGET_PACKAGES"),
            Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".nuget", "packages"),
            Path.Combine(Path.GetPathRoot(Environment.SystemDirectory) ?? "C:\\", ".tools", ".nuget", "packages")
        };

        foreach (var candidate in candidates.Where(static c => !string.IsNullOrWhiteSpace(c)))
        {
            var fullPath = Path.GetFullPath(candidate!);
            if (seen.Add(fullPath))
            {
                yield return fullPath;
            }
        }
    }

    private static string? FindLatestGrpcToolsDirectory()
    {
        foreach (var packageRoot in EnumerateNuGetPackageRoots())
        {
            var grpcToolsRoot = Path.Combine(packageRoot, "grpc.tools");
            if (!Directory.Exists(grpcToolsRoot))
            {
                continue;
            }

            return Directory
                .EnumerateDirectories(grpcToolsRoot)
                .OrderDescending()
                .FirstOrDefault();
        }

        return null;
    }

    private static string? FindOnPath(string commandName)
    {
        var pathValue = Environment.GetEnvironmentVariable("PATH");
        if (string.IsNullOrWhiteSpace(pathValue))
        {
            return null;
        }

        var executableNames = new List<string> { commandName };
        if (OperatingSystem.IsWindows() && !commandName.EndsWith(".exe", StringComparison.OrdinalIgnoreCase))
        {
            executableNames.Add($"{commandName}.exe");
        }

        foreach (var pathSegment in pathValue.Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
        {
            foreach (var executableName in executableNames)
            {
                var candidate = Path.Combine(pathSegment, executableName);
                if (File.Exists(candidate))
                {
                    return candidate;
                }
            }
        }

        return null;
    }
}
