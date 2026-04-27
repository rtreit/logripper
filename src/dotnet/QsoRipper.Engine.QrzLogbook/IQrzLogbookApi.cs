using QsoRipper.Domain;

namespace QsoRipper.Engine.QrzLogbook;

/// <summary>
/// Abstracts QRZ Logbook HTTP operations so the sync engine can be tested without network access.
/// </summary>
public interface IQrzLogbookApi
{
    /// <summary>
    /// Fetch QSOs from the remote QRZ logbook.
    /// When <paramref name="sinceDateYmd"/> is non-null the request includes <c>OPTION=MODSINCE:{date}</c>.
    /// </summary>
    Task<List<QsoRecord>> FetchQsosAsync(string? sinceDateYmd);

    /// <summary>
    /// Upload a single QSO to QRZ via the INSERT action.
    /// Returns the QRZ-assigned LOGID on success.
    /// </summary>
    /// <param name="qso">The QSO record to upload.</param>
    /// <param name="bookOwner">
    /// Optional QRZ logbook owner callsign (from a fresh STATUS call, falling back to
    /// cached <c>SyncMetadata.QrzLogbookOwner</c>). When provided and different from
    /// <c>qso.StationCallsign</c>, the upload payload's <c>STATION_CALLSIGN</c> is
    /// rewritten to the owner so QRZ accepts QSOs logged under a previous callsign.
    /// The local QSO is never modified.
    /// </param>
    Task<string> UploadQsoAsync(QsoRecord qso, string? bookOwner = null);

    /// <summary>
    /// Update an existing QSO on QRZ via the REPLACE action.
    /// The <paramref name="qso"/> must have a non-empty <see cref="QsoRecord.QrzLogid"/>
    /// that identifies the remote record to overwrite.
    /// Returns the QRZ LOGID on success.
    /// </summary>
    /// <param name="qso">The QSO record to update.</param>
    /// <param name="bookOwner">
    /// Optional QRZ logbook owner callsign — same semantics as
    /// <see cref="UploadQsoAsync"/>: rewrites the upload payload's
    /// <c>STATION_CALLSIGN</c> when the QSO was logged under a previous callsign.
    /// </param>
    Task<string> UpdateQsoAsync(QsoRecord qso, string? bookOwner = null);

    /// <summary>
    /// Calls the QRZ Logbook <c>STATUS</c> action and returns the authoritative
    /// server-side QSO count and owner callsign. Used in Phase 3 of sync to
    /// keep <c>SyncMetadata</c> aligned with what QRZ actually has.
    /// </summary>
    Task<QrzLogbookStatus> GetStatusAsync();

    /// <summary>
    /// Delete a remote QSO by its QRZ logid via the <c>DELETE</c> action.
    /// Implementations must treat QRZ "not found"-style failures as success
    /// so the queued-remote-delete sync loop is idempotent.
    /// </summary>
    Task DeleteQsoAsync(string logid);
}

/// <summary>
/// Result of a successful QRZ Logbook <c>STATUS</c> call.
/// </summary>
/// <param name="Owner">QRZ logbook owner callsign (from <c>CALLSIGN</c>, falling back to <c>OWNER</c>).</param>
/// <param name="QsoCount">Total QSO count reported by QRZ.</param>
public readonly record struct QrzLogbookStatus(string Owner, uint QsoCount);
