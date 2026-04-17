namespace QsoRipper.EngineSelection;

public sealed record EngineRuntimeDiscoveryOptions
{
    public string RuntimeDirectory { get; init; } = Path.Combine(".", "artifacts", "run");

    public bool ValidateTcpReachability { get; init; } = true;

    public TimeSpan TcpProbeTimeout { get; init; } = TimeSpan.FromMilliseconds(500);
}
