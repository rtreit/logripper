namespace QsoRipper.EngineSelection;

public sealed record EngineCommand(
    string FilePath,
    IReadOnlyList<string> Arguments);
