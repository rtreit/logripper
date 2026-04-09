namespace LogRipper.DebugHost.Models;

public sealed class DebugWorkbenchOptions
{
    public const string SectionName = "DebugWorkbench";

    public string DefaultEngineEndpoint { get; set; } = "http://localhost:50051";

    public int ProbeTimeoutSeconds { get; set; } = 3;

    public int CommandTimeoutSeconds { get; set; } = 300;
}
