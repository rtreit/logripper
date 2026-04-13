namespace QsoRipper.DebugHost.Models;

internal sealed record TransportProbeResult(
    bool IsSuccess,
    EngineProbeStage Stage,
    string Summary,
    string NextAction,
    DateTimeOffset AttemptedAtUtc,
    string Endpoint);
