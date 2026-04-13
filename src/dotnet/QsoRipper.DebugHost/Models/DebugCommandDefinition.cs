namespace QsoRipper.DebugHost.Models;

internal sealed record DebugCommandDefinition(
    string Key,
    string DisplayName,
    string Description,
    string FileName,
    string Arguments,
    string WorkingDirectory,
    bool RequiresProtoc = false,
    string? RequiredTool = null);
