namespace QsoRipper.Gui.ViewModels;

/// <summary>
/// Represents a selectable UTC timestamp display format for the QSO grid.
/// </summary>
internal sealed record TimestampFormatOption(string Label, string FormatString)
{
    /// <summary>All available format options, ordered for cycling.</summary>
    public static TimestampFormatOption[] All { get; } =
    [
        new("yy-MM-dd", "yy-MM-dd HH:mm"),
        new("yyyy-MM-dd", "yyyy-MM-dd HH:mm"),
        new("MM/dd/yyyy", "MM/dd/yyyy HH:mm"),
        new("dd-MM-yyyy", "dd-MM-yyyy HH:mm"),
    ];

    /// <summary>The default format used when no preference is set.</summary>
    public static TimestampFormatOption Default => All[0];

    /// <summary>
    /// Returns the next option in the cycle after the given format string,
    /// wrapping to the first option if the current is not found or is last.
    /// </summary>
    public static TimestampFormatOption CycleNext(string currentFormatString)
    {
        var options = All;
        var currentIndex = Array.FindIndex(options, o =>
            string.Equals(o.FormatString, currentFormatString, StringComparison.Ordinal));
        var nextIndex = (currentIndex + 1) % options.Length;
        return options[nextIndex];
    }

    /// <summary>
    /// Finds the option matching a format string, or returns <see cref="Default"/>.
    /// </summary>
    public static TimestampFormatOption FindOrDefault(string? formatString) =>
        Array.Find(All, o =>
            string.Equals(o.FormatString, formatString, StringComparison.Ordinal))
        ?? Default;
}
