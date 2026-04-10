namespace LogRipper.DebugHost.Models;

internal sealed class DebugWorkbenchOptions
{
    public const string SectionName = "DebugWorkbench";

    public string DefaultEngineEndpoint { get; set; } = "http://localhost:50051";

    public string DefaultEngineStorageBackend { get; set; } = "memory";

    public string DefaultEngineSqlitePath { get; set; } = @".\data\logripper.db";

    public int ProbeTimeoutSeconds { get; set; } = 3;

    public int CommandTimeoutSeconds { get; set; } = 300;
}
