namespace QsoRipper.DebugHost.Models;

internal sealed class DebugWorkbenchOptions
{
    public const string SectionName = "DebugWorkbench";

    public string DefaultEngineEndpoint { get; set; } = "http://127.0.0.1:50051";

    public string DefaultEngineStorageBackend { get; set; } = "memory";

    public string DefaultEngineSqlitePath { get; set; } = @".\data\qsoripper.db";

    public int ProbeTimeoutSeconds { get; set; } = 3;

    public int CommandTimeoutSeconds { get; set; } = 300;
}
