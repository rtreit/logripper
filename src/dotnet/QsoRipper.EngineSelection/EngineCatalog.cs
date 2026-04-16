using System.Diagnostics.CodeAnalysis;
using System.IO;
using System.Linq;

namespace QsoRipper.EngineSelection;

public static class EngineCatalog
{
    public const string EngineProfileEnvironmentVariable = "QSORIPPER_ENGINE";
    public const string LegacyEngineProfileEnvironmentVariable = "QSORIPPER_ENGINE_IMPLEMENTATION";
    public const string EndpointEnvironmentVariable = "QSORIPPER_ENDPOINT";

    private static readonly string StartScriptPath = Path.Combine(".", "start-qsoripper.ps1");

    private static readonly IReadOnlyList<EngineTargetProfile> BuiltInProfiles =
    [
        new(
            KnownEngineProfiles.LocalRust,
            "rust-tonic",
            "QsoRipper Rust Engine",
            "http://127.0.0.1:50051",
            [KnownEngineProfiles.LocalRust, "rust", "rust-tonic"],
            new EngineLaunchRecipe(
                Path.Combine(".", "artifacts", "run", "rust-engine.json"),
                SupportsStorageSession: true,
                new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
                {
                    ["QSORIPPER_STORAGE_BACKEND"] = "{storageBackend}",
                    ["QSORIPPER_SQLITE_PATH"] = "{enginePersistenceLocation}",
                },
                new EngineCommand(
                    "pwsh",
                    [
                        "-File",
                        StartScriptPath,
                        "-Engine",
                        KnownEngineProfiles.LocalRust,
                        "-ListenAddress",
                        "{listenAddress}",
                        "-Storage",
                        "{storageBackend}",
                        "-PersistenceLocation",
                        "{persistenceLocation}",
                        "-ConfigPath",
                        "{configPath}"
                    ]))),
        new(
            KnownEngineProfiles.LocalDotNet,
            "dotnet-aspnet",
            "QsoRipper .NET Engine",
            "http://127.0.0.1:50052",
            [KnownEngineProfiles.LocalDotNet, "dotnet", "dotnet-aspnet", "managed"],
            new EngineLaunchRecipe(
                Path.Combine(".", "artifacts", "run", "dotnet-engine.json"),
                SupportsStorageSession: true,
                new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase)
                {
                    ["QSORIPPER_STORAGE_BACKEND"] = "{storageBackend}",
                    ["QSORIPPER_STORAGE_PATH"] = "{enginePersistenceLocation}",
                },
                new EngineCommand(
                    "pwsh",
                    [
                        "-File",
                        StartScriptPath,
                        "-Engine",
                        KnownEngineProfiles.LocalDotNet,
                        "-ListenAddress",
                        "{listenAddress}",
                        "-Storage",
                        "{storageBackend}",
                        "-PersistenceLocation",
                        "{persistenceLocation}",
                        "-ConfigPath",
                        "{configPath}"
                    ]))),
    ];

    public static EngineTargetProfile DefaultProfile => RustProfile;

    public static EngineTargetProfile RustProfile => GetProfile(KnownEngineProfiles.LocalRust);

    public static EngineTargetProfile DotNetProfile => GetProfile(KnownEngineProfiles.LocalDotNet);

    public static string DefaultRustEndpoint => RustProfile.DefaultEndpoint;

    public static string DefaultDotNetEndpoint => DotNetProfile.DefaultEndpoint;

    public static IReadOnlyList<EngineTargetProfile> LocalProfiles => BuiltInProfiles;

    public static EngineTargetProfile GetProfile(string profileId)
    {
        if (TryResolveProfile(profileId, out var profile))
        {
            return profile;
        }

        throw new ArgumentOutOfRangeException(nameof(profileId), profileId, "Unknown engine profile.");
    }

    public static string GetKnownProfileList()
    {
        return string.Join(
            ", ",
            BuiltInProfiles
                .SelectMany(profile => profile.Aliases.Prepend(profile.ProfileId))
                .Distinct(StringComparer.OrdinalIgnoreCase));
    }

    public static EngineTargetProfile ResolveProfile(string? value = null)
    {
        return TryResolveProfile(
            value
            ?? Environment.GetEnvironmentVariable(EngineProfileEnvironmentVariable)
            ?? Environment.GetEnvironmentVariable(LegacyEngineProfileEnvironmentVariable),
            out var profile)
            ? profile
            : DefaultProfile;
    }

    public static string ResolveEndpoint(
        EngineTargetProfile profile,
        string? explicitEndpoint = null,
        string? environmentEndpoint = null)
    {
        ArgumentNullException.ThrowIfNull(profile);

        var endpoint = string.IsNullOrWhiteSpace(explicitEndpoint)
            ? environmentEndpoint ?? Environment.GetEnvironmentVariable(EndpointEnvironmentVariable)
            : explicitEndpoint;

        return string.IsNullOrWhiteSpace(endpoint)
            ? profile.DefaultEndpoint
            : endpoint.Trim();
    }

    public static bool IsDefaultEndpoint(string? endpoint, EngineTargetProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        return string.Equals(
            endpoint?.Trim(),
            profile.DefaultEndpoint,
            StringComparison.OrdinalIgnoreCase);
    }

    public static int GetDefaultPort(EngineTargetProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        return new Uri(profile.DefaultEndpoint, UriKind.Absolute).Port;
    }

    public static bool TryResolveProfile(
        string? value,
        [NotNullWhen(true)] out EngineTargetProfile? profile)
    {
        profile = BuiltInProfiles.FirstOrDefault(candidate => candidate.Matches(value));
        return profile is not null;
    }
}
