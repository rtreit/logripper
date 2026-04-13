using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Models;

internal sealed record LookupInvocationResult(
    LookupRequest Request,
    IReadOnlyList<LookupResult> Responses,
    string? ErrorMessage,
    string InvocationMode,
    DateTimeOffset CompletedAtUtc)
{
    public bool Succeeded => string.IsNullOrWhiteSpace(ErrorMessage);

    public IReadOnlyList<DebugHttpExchange> DebugHttpExchanges =>
        Responses.SelectMany(static response => response.DebugHttpExchanges).ToArray();
}
