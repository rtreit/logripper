using QsoRipper.Domain;

namespace QsoRipper.Engine.Storage;

/// <summary>
/// Page of prior QSOs for a worked callsign plus the unbounded total active-row
/// count regardless of the requested limit.
/// </summary>
public sealed class QsoHistoryPage
{
    public QsoHistoryPage(IReadOnlyList<QsoRecord> entries, int total)
    {
        Entries = entries ?? throw new ArgumentNullException(nameof(entries));
        Total = total;
    }

    public static QsoHistoryPage Empty { get; } = new(Array.Empty<QsoRecord>(), 0);

    /// <summary>Most-recent-first prior QSOs, capped by the caller's limit.</summary>
    public IReadOnlyList<QsoRecord> Entries { get; }

    /// <summary>Total active prior QSOs regardless of the requested limit.</summary>
    public int Total { get; }
}
