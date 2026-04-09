using LogRipper.Domain;

namespace LogRipper.DebugHost.Models;

public sealed record LookupInvocationResult(
    LookupRequest Request,
    IReadOnlyList<LookupResult> Responses,
    string? ErrorMessage,
    bool WasStreaming,
    DateTimeOffset CompletedAtUtc)
{
    public bool Succeeded => string.IsNullOrWhiteSpace(ErrorMessage);
}
