using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Models;

internal sealed record StorageSmokeTestResult(
    QsoRecord RequestedQso,
    LogQsoResponse? LogResponse,
    GetQsoResponse? LoadedResponse,
    IReadOnlyList<QsoRecord> ListedQsos,
    GetSyncStatusResponse? SyncStatus,
    DeleteQsoResponse? DeleteResponse,
    bool RetainedRecord,
    bool DeleteVerified,
    string? ErrorMessage,
    DateTimeOffset CompletedAtUtc)
{
    public bool Succeeded => string.IsNullOrWhiteSpace(ErrorMessage);

    public string? LocalId => LogResponse?.LocalId;
}
