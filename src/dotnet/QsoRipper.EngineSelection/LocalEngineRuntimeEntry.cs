namespace QsoRipper.EngineSelection;

public sealed record LocalEngineRuntimeEntry(
    EngineTargetProfile Profile,
    string Endpoint,
    string ListenAddress,
    int ProcessId,
    bool IsProcessAlive,
    bool IsTransportReachable,
    DateTimeOffset? StartedAtUtc,
    string StatePath)
{
    public bool IsRunning => IsProcessAlive && IsTransportReachable;
}
