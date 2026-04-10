namespace LogRipper.DebugHost.Models;

internal sealed record TransportProbeResult(
    bool IsSuccess,
    string Summary,
    DateTimeOffset AttemptedAtUtc,
    string Endpoint);
