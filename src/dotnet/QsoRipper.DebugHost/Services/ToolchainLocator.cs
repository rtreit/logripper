using QsoRipper.DebugHost.Models;

namespace QsoRipper.DebugHost.Services;

internal sealed class ToolchainLocator
{
    private IReadOnlyList<ToolAvailability>? _cachedAvailability;

#pragma warning disable CA1822 // Mark members as static
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
#pragma warning restore CA1822 // Mark members as static

    public IReadOnlyList<ToolAvailability> GetToolAvailability()
    {
        if (_cachedAvailability is not null)
        {
            return _cachedAvailability;
        }

        var dotnet = FindDotnet();
        var cargo = FindCargo();
        var buf = FindBuf();
        var protoc = FindProtoc();

        _cachedAvailability =
        [
            new("dotnet", dotnet is not null, dotnet),
            new("cargo", cargo is not null, cargo),
            new("buf", buf is not null, buf),
            new("protoc", protoc is not null, protoc)
        ];

        return _cachedAvailability;
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
