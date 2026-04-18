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
    Task<string> UploadQsoAsync(QsoRecord qso);

    /// <summary>
    /// Update an existing QSO on QRZ via the REPLACE action.
    /// The <paramref name="qso"/> must have a non-empty <see cref="QsoRecord.QrzLogid"/>
    /// that identifies the remote record to overwrite.
    /// Returns the QRZ LOGID on success.
    /// </summary>
    Task<string> UpdateQsoAsync(QsoRecord qso);
}
