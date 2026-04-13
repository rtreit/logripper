using QsoRipper.Domain;

namespace QsoRipper.DebugHost.Models;

internal sealed record QsoViewerResult(
    IReadOnlyList<QsoRecord> Records,
    uint TotalFetched,
    string? ErrorMessage,
    DateTimeOffset CompletedAtUtc)
{
    public bool Succeeded => string.IsNullOrWhiteSpace(ErrorMessage);
}
