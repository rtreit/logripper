using System;

namespace QsoRipper.EngineSelection;

/// <summary>
/// Formats QSO durations consistently across all .NET surfaces (CLI, GUI, DebugHost).
///
/// Mirrors <c>qsoripper-core::domain::duration::format_duration_seconds</c> in the Rust
/// engine so output is identical regardless of which engine or client renders the value.
///
/// Format conventions (compact, suitable for log columns):
/// <list type="bullet">
///   <item><c>&lt; 1m</c> -> <c>"Ns"</c>   e.g. <c>"45s"</c></item>
///   <item><c>&lt; 1h</c> -> <c>"Mm SSs"</c> e.g. <c>"2m 35s"</c></item>
///   <item><c>&gt;= 1h</c> -> <c>"Hh MMm"</c> e.g. <c>"1h 12m"</c></item>
/// </list>
///
/// Returns <c>null</c> when the duration is not strictly positive so callers can render
/// <c>"—"</c> or omit the column entirely.
///
/// See <see href="https://github.com/rtreit/qsoripper/issues/201"/>.
/// </summary>
public static class QsoDurationFormatter
{
    /// <summary>
    /// Format a positive duration in whole seconds. Returns <c>null</c> when
    /// <paramref name="seconds"/> is zero or negative.
    /// </summary>
    public static string? FormatSeconds(long seconds)
    {
        if (seconds <= 0)
        {
            return null;
        }

        var hours = seconds / 3600;
        var minutes = (seconds % 3600) / 60;
        var secs = seconds % 60;

        if (hours > 0)
        {
            return $"{hours}h {minutes:D2}m";
        }

        if (minutes > 0)
        {
            return $"{minutes}m {secs:D2}s";
        }

        return $"{secs}s";
    }

    /// <summary>
    /// Compute and format the QSO duration when both timestamps are present and
    /// <paramref name="end"/> is strictly after <paramref name="start"/>. Returns
    /// <c>null</c> otherwise.
    /// </summary>
    public static string? Format(DateTimeOffset? start, DateTimeOffset? end)
    {
        if (start is null || end is null)
        {
            return null;
        }

        var delta = end.Value - start.Value;
        var seconds = (long)delta.TotalSeconds;
        return FormatSeconds(seconds);
    }
}
