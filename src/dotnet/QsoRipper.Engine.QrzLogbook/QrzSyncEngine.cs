using System.Globalization;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Engine.Storage;

#pragma warning disable CA1031 // Do not catch general exception types — sync must be resilient to partial failures

namespace QsoRipper.Engine.QrzLogbook;

/// <summary>
/// Orchestrates a 3-phase bidirectional sync between the local logbook and QRZ:
/// <list type="number">
///   <item><description>Download remote QSOs and merge/insert locally.</description></item>
///   <item><description>Upload pending local QSOs to QRZ.</description></item>
///   <item><description>Update sync metadata with the current timestamp.</description></item>
/// </list>
/// </summary>
public sealed class QrzSyncEngine
{
    /// <summary>Extra-field key that QRZ ADIF uses for the logbook record ID.</summary>
    private const string QrzLogidExtraField = "APP_QRZ_LOGID";

    /// <summary>Alternate extra-field key used by some QRZ ADIF exports.</summary>
    private const string QrzLogidAltExtraField = "APP_QRZLOG_LOGID";

    /// <summary>Maximum time difference (seconds) for fuzzy timestamp matching.</summary>
    private const long TimestampToleranceSeconds = 60;

    private readonly IQrzLogbookApi _client;

    /// <summary>
    /// Initializes a new instance of the <see cref="QrzSyncEngine"/> class.
    /// </summary>
    public QrzSyncEngine(IQrzLogbookApi client)
    {
        ArgumentNullException.ThrowIfNull(client);
        _client = client;
    }

    /// <summary>
    /// Execute a full sync cycle against the given logbook store.
    /// </summary>
    /// <param name="store">The logbook store to sync.</param>
    /// <param name="fullSync">When <c>true</c>, re-fetches all QRZ records instead of incremental.</param>
    /// <returns>A <see cref="SyncResult"/> with counts and any error summary.</returns>
    public async Task<SyncResult> ExecuteSyncAsync(ILogbookStore store, bool fullSync)
    {
        ArgumentNullException.ThrowIfNull(store);

        var errors = new List<string>();
        uint downloaded = 0;
        uint uploaded = 0;
        uint conflicts = 0;

        // ---------------------------------------------------------------
        // Phase 1 — Download from QRZ
        // ---------------------------------------------------------------

        SyncMetadata metadata;
        try
        {
            metadata = await store.GetSyncMetadataAsync().ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            metadata = new SyncMetadata();
            errors.Add($"Failed to read sync metadata: {ex.Message}");
        }

        IReadOnlyList<QsoRecord> localQsos;
        try
        {
            localQsos = await store.ListQsosAsync(new QsoListQuery { Sort = QsoSortOrder.OldestFirst }).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            return new SyncResult
            {
                ErrorSummary = $"Failed to load local QSOs: {ex.Message}",
            };
        }

        // Force full fetch when local logbook is empty (first sync or data loss recovery).
        var sinceDate = (fullSync || localQsos.Count == 0) ? null : FormatSinceDate(metadata);

        List<QsoRecord> remoteQsos;
        try
        {
            remoteQsos = await _client.FetchQsosAsync(sinceDate).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            return new SyncResult
            {
                ErrorSummary = $"Failed to fetch QSOs from QRZ: {ex.Message}",
            };
        }

        // Build lookup indexes over local QSOs.
        var (byLogid, byKey) = BuildLocalIndexes(localQsos);

        foreach (var remote in remoteQsos)
        {
            // Ghost filter: skip QSOs with empty callsign or missing timestamp.
            if (string.IsNullOrWhiteSpace(remote.WorkedCallsign) || remote.UtcTimestamp is null)
            {
                continue;
            }

            var remoteLogid = ExtractQrzLogid(remote);

            // Try match by QRZ logid first, then fuzzy match.
            var localMatch = remoteLogid is not null && byLogid.TryGetValue(remoteLogid, out var logidMatch)
                ? logidMatch
                : FuzzyMatch(remote, byKey);

            if (localMatch is null)
            {
                // New remote QSO — insert locally.
                var newQso = remote.Clone();
                if (string.IsNullOrEmpty(newQso.LocalId))
                {
                    newQso.LocalId = Guid.NewGuid().ToString();
                }

                newQso.SyncStatus = SyncStatus.Synced;
                if (remoteLogid is not null)
                {
                    newQso.QrzLogid = remoteLogid;
                }

                try
                {
                    await store.InsertQsoAsync(newQso).ConfigureAwait(false);
                    downloaded++;

                    // Keep indexes current for subsequent iterations.
                    if (remoteLogid is not null)
                    {
                        byLogid[remoteLogid] = newQso;
                    }

                    var key = MakeFuzzyKey(newQso);
                    if (!byKey.TryGetValue(key, out var list))
                    {
                        list = [];
                        byKey[key] = list;
                    }

                    list.Add(newQso);
                }
                catch (Exception ex)
                {
                    errors.Add($"Insert failed for {remote.WorkedCallsign}: {ex.Message}");
                }
            }
            else
            {
                // Matched existing QSO — merge.
                try
                {
                    var merged = MergeRemoteIntoLocal(localMatch, remote, remoteLogid);
                    if (await store.UpdateQsoAsync(merged).ConfigureAwait(false))
                    {
                        downloaded++;
                    }
                }
                catch (Exception ex)
                {
                    errors.Add($"Merge failed for {remote.WorkedCallsign}: {ex.Message}");
                }
            }
        }

        // ---------------------------------------------------------------
        // Phase 2 — Upload pending local QSOs
        // ---------------------------------------------------------------

        IReadOnlyList<QsoRecord> pendingQsos;
        try
        {
            var allQsos = await store.ListQsosAsync(new QsoListQuery { Sort = QsoSortOrder.OldestFirst }).ConfigureAwait(false);
            pendingQsos = allQsos
                .Where(q => q.SyncStatus is SyncStatus.LocalOnly or SyncStatus.Modified)
                .ToList();
        }
        catch (Exception ex)
        {
            errors.Add($"Failed to list pending QSOs for upload: {ex.Message}");
            pendingQsos = [];
        }

        foreach (var qso in pendingQsos)
        {
            try
            {
                var logid = qso.SyncStatus == SyncStatus.Modified && !string.IsNullOrWhiteSpace(qso.QrzLogid)
                    ? await _client.UpdateQsoAsync(qso).ConfigureAwait(false)
                    : await _client.UploadQsoAsync(qso).ConfigureAwait(false);
                var synced = qso.Clone();
                synced.QrzLogid = logid;
                synced.SyncStatus = SyncStatus.Synced;
                try
                {
                    await store.UpdateQsoAsync(synced).ConfigureAwait(false);
                }
                catch (Exception ex)
                {
                    errors.Add($"Upload succeeded for {qso.WorkedCallsign} but local update failed: {ex.Message}");
                }

                uploaded++;
            }
            catch (Exception ex)
            {
                errors.Add($"Upload failed for {qso.WorkedCallsign}: {ex.Message}");
            }
        }

        // ---------------------------------------------------------------
        // Phase 3 — Update sync metadata
        // ---------------------------------------------------------------

        try
        {
            await store.UpsertSyncMetadataAsync(new SyncMetadata
            {
                QrzQsoCount = metadata.QrzQsoCount,
                LastSync = DateTimeOffset.UtcNow,
                QrzLogbookOwner = metadata.QrzLogbookOwner,
            }).ConfigureAwait(false);
        }
        catch (Exception ex)
        {
            errors.Add($"Failed to update sync metadata: {ex.Message}");
        }

        return new SyncResult
        {
            DownloadedCount = downloaded,
            UploadedCount = uploaded,
            ConflictCount = conflicts,
            ErrorSummary = errors.Count > 0 ? string.Join("; ", errors) : null,
        };
    }

    // -- Matching helpers ---------------------------------------------------

    private static (Dictionary<string, QsoRecord> ByLogid, Dictionary<(string Call, Band Band, Mode Mode), List<QsoRecord>> ByKey)
        BuildLocalIndexes(IReadOnlyList<QsoRecord> localQsos)
    {
        var byLogid = new Dictionary<string, QsoRecord>(StringComparer.OrdinalIgnoreCase);
        var byKey = new Dictionary<(string, Band, Mode), List<QsoRecord>>();

        foreach (var qso in localQsos)
        {
            var logid = ExtractQrzLogid(qso);
            if (logid is not null)
            {
                byLogid.TryAdd(logid, qso);
            }

            var key = MakeFuzzyKey(qso);
            if (!byKey.TryGetValue(key, out var list))
            {
                list = [];
                byKey[key] = list;
            }

            list.Add(qso);
        }

        return (byLogid, byKey);
    }

    private static (string Call, Band Band, Mode Mode) MakeFuzzyKey(QsoRecord qso) =>
        (qso.WorkedCallsign.ToUpperInvariant(), qso.Band, qso.Mode);

    private static QsoRecord? FuzzyMatch(QsoRecord remote, Dictionary<(string, Band, Mode), List<QsoRecord>> byKey)
    {
        var key = MakeFuzzyKey(remote);
        if (!byKey.TryGetValue(key, out var candidates))
        {
            return null;
        }

        var remoteTs = remote.UtcTimestamp?.Seconds ?? 0;
        return candidates.Find(local =>
            local.UtcTimestamp is not null
            && Math.Abs(local.UtcTimestamp.Seconds - remoteTs) <= TimestampToleranceSeconds);
    }

    /// <summary>
    /// Extract the QRZ logbook record ID from a QSO, checking the dedicated field first,
    /// then falling back to common extra-field keys.
    /// </summary>
    internal static string? ExtractQrzLogid(QsoRecord qso)
    {
        if (qso.HasQrzLogid && !string.IsNullOrWhiteSpace(qso.QrzLogid))
        {
            return qso.QrzLogid;
        }

        if (qso.ExtraFields.TryGetValue(QrzLogidAltExtraField, out var alt) && !string.IsNullOrWhiteSpace(alt))
        {
            return alt;
        }

        if (qso.ExtraFields.TryGetValue(QrzLogidExtraField, out var logid) && !string.IsNullOrWhiteSpace(logid))
        {
            return logid;
        }

        return null;
    }

    /// <summary>
    /// Merge remote QSO data into an existing local QSO. Preserves the local ID and updates sync metadata.
    /// </summary>
    private static QsoRecord MergeRemoteIntoLocal(QsoRecord local, QsoRecord remote, string? remoteLogid)
    {
        // For already-synced records, remote wins (overwrite with fresh remote data).
        // For local-only records, link to remote and mark synced to avoid duplicate upload.
        var merged = local.SyncStatus == SyncStatus.Synced ? remote.Clone() : local.Clone();
        merged.LocalId = local.LocalId;
        merged.SyncStatus = SyncStatus.Synced;

        // Preserve or assign logid.
        merged.QrzLogid = remoteLogid ?? local.QrzLogid;

        return merged;
    }

    private static string? FormatSinceDate(SyncMetadata metadata)
    {
        if (metadata.LastSync is not { } lastSync)
        {
            return null;
        }

        return lastSync.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
    }
}
