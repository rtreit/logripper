namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Parses slash-modified callsigns into base callsign and modifier components.
/// </summary>
public static class CallsignParser
{
    /// <summary>Parse a callsign that may contain a slash modifier.</summary>
    /// <remarks>
    /// Examples:
    /// <list type="bullet">
    /// <item><c>VP2E/K7ABC</c> → base=K7ABC, modifier=VP2E, position=Prefix</item>
    /// <item><c>K7ABC/M</c> → base=K7ABC, modifier=M, position=Suffix</item>
    /// <item><c>K7ABC</c> → base=K7ABC, modifier=null, position=None</item>
    /// </list>
    /// </remarks>
    public static ParsedCallsign Parse(string callsign)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);
        var normalized = callsign.Trim().ToUpperInvariant();

        var slashIndex = normalized.IndexOf('/', StringComparison.Ordinal);
        if (slashIndex < 0)
        {
            return new ParsedCallsign(normalized, normalized, null, ModifierPosition.None);
        }

        var left = normalized[..slashIndex];
        var right = normalized[(slashIndex + 1)..];

        if (string.IsNullOrEmpty(left) || string.IsNullOrEmpty(right))
        {
            // Degenerate slash at start/end — treat entire string as base.
            return new ParsedCallsign(normalized, normalized, null, ModifierPosition.None);
        }

        // Heuristic: if the right part is shorter, it's a suffix modifier (K7ABC/M, K7ABC/P).
        // If the left part is shorter, it's a prefix modifier (VP2E/K7ABC).
        // If equal length, treat the left as a prefix override (e.g. EA8/K7ABC).
        if (right.Length <= left.Length && right.Length <= 3)
        {
            // Suffix modifier: K7ABC/M → base=K7ABC, modifier=M
            return new ParsedCallsign(normalized, left, right, ModifierPosition.Suffix);
        }

        // Prefix modifier: VP2E/K7ABC → base=K7ABC, modifier=VP2E
        return new ParsedCallsign(normalized, right, left, ModifierPosition.Prefix);
    }
}

/// <summary>Result of parsing a potentially slash-modified callsign.</summary>
public sealed record ParsedCallsign(
    string OriginalCallsign,
    string BaseCallsign,
    string? Modifier,
    ModifierPosition Position);

/// <summary>Position of a slash modifier in a callsign.</summary>
public enum ModifierPosition
{
    None,
    Prefix,
    Suffix,
}
