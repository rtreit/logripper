using QsoRipper.EngineSelection;

namespace QsoRipper.Cli;

internal sealed record CliArguments(
    string Command,
    string Endpoint,
    EngineTargetProfile EngineProfile,
    bool ShowHelp = false,
    string? Error = null,
    string? Callsign = null,
    bool SkipCache = false,
    bool JsonOutput = false,
    bool Refresh = false,
    bool Force = false,
    bool SetupStatus = false,
    bool SetupFromEnv = false,
    string[] RemainingArgs = default!)
{
    public string[] RemainingArgs { get; init; } = RemainingArgs ?? [];
}
