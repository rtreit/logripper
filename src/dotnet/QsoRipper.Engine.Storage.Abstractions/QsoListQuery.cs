using QsoRipper.Domain;

namespace QsoRipper.Engine.Storage;

/// <summary>
/// Describes filters, sorting, and pagination for a QSO list query.
/// All filter properties are optional — <c>null</c> means no filter for that field.
/// </summary>
public sealed record QsoListQuery
{
    /// <summary>Only return QSOs with a timestamp strictly after this value.</summary>
    public DateTimeOffset? After { get; init; }

    /// <summary>Only return QSOs with a timestamp strictly before this value.</summary>
    public DateTimeOffset? Before { get; init; }

    /// <summary>Substring match on the worked callsign (case-insensitive).</summary>
    public string? CallsignFilter { get; init; }

    /// <summary>Exact match on the QSO band.</summary>
    public Band? BandFilter { get; init; }

    /// <summary>Exact match on the QSO mode.</summary>
    public Mode? ModeFilter { get; init; }

    /// <summary>Exact match on the contest identifier (case-insensitive).</summary>
    public string? ContestId { get; init; }

    /// <summary>Maximum number of records to return. <c>null</c> means no limit.</summary>
    public int? Limit { get; init; }

    /// <summary>Number of records to skip before returning results.</summary>
    public int Offset { get; init; }

    /// <summary>Sort order for the result set. Defaults to <see cref="QsoSortOrder.NewestFirst"/>.</summary>
    public QsoSortOrder Sort { get; init; } = QsoSortOrder.NewestFirst;
}

/// <summary>Sort order for QSO list queries.</summary>
public enum QsoSortOrder
{
    /// <summary>Most recent QSOs first (descending timestamp).</summary>
    NewestFirst,

    /// <summary>Oldest QSOs first (ascending timestamp).</summary>
    OldestFirst,
}
