namespace LogRipper.DebugHost.Models;

public sealed record TransportProbeResult(
    bool IsSuccess,
    string Summary,
    DateTimeOffset AttemptedAtUtc,
    string Endpoint);
