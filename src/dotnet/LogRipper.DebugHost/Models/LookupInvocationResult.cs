using LogRipper.Domain;

namespace LogRipper.DebugHost.Models;

internal sealed record LookupInvocationResult(
    LookupRequest Request,
    IReadOnlyList<LookupResult> Responses,
    string? ErrorMessage,
    bool WasStreaming,
    DateTimeOffset CompletedAtUtc)
{
    public bool Succeeded => string.IsNullOrWhiteSpace(ErrorMessage);
}
