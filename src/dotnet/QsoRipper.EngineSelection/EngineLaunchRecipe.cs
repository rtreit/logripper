namespace QsoRipper.EngineSelection;

public sealed record EngineLaunchRecipe(
    string DefaultConfigPath,
    bool SupportsStorageSession,
    IReadOnlyDictionary<string, string> EnvironmentTemplates,
    EngineCommand BuildCommand,
    EngineCommand LaunchCommand);
