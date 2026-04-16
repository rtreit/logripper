namespace QsoRipper.DebugHost.Models;

internal sealed class DebugWorkbenchOptions
{
    public const string SectionName = "DebugWorkbench";

    public string DefaultEngineProfile { get; set; } = "rust";

    public string DefaultEngineImplementation { get; set; } = string.Empty;

    public string DefaultEngineEndpoint { get; set; } = string.Empty;

    public string DefaultEngineStorageBackend { get; set; } = "memory";

    public string DefaultEngineSqlitePath { get; set; } = @".\data\qsoripper.db";

    public int ProbeTimeoutSeconds { get; set; } = 3;

    public int CommandTimeoutSeconds { get; set; } = 300;
}
