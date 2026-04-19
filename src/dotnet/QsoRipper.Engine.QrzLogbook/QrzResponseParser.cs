namespace QsoRipper.Engine.QrzLogbook;

/// <summary>
/// Parses key-value response bodies from the QRZ Logbook API.
/// </summary>
internal static class QrzResponseParser
{
    /// <summary>
    /// Parse a QRZ key-value response body (<c>KEY=VALUE&amp;KEY=VALUE</c>) into a dictionary.
    /// Keys are upper-cased for consistent lookup.
    /// </summary>
    /// <remarks>
    /// This must only be used on non-ADIF responses (e.g. INSERT results).
    /// FETCH responses embed raw ADIF after <c>ADIF=</c> and cannot be split naïvely on <c>&amp;</c>.
    /// </remarks>
    internal static Dictionary<string, string> ParseKeyValueResponse(string body)
    {
        var map = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        foreach (var pair in body.Trim().Split('&'))
        {
            var eqIndex = pair.IndexOf('=', StringComparison.Ordinal);
            if (eqIndex >= 0)
            {
                var key = pair[..eqIndex].ToUpperInvariant();
                var value = pair[(eqIndex + 1)..];
                map[key] = value;
            }
        }

        return map;
    }

    /// <summary>
    /// Validates that a parsed QRZ response has <c>RESULT=OK</c>.
    /// </summary>
    /// <returns>The validated map for further field extraction.</returns>
    /// <exception cref="QrzLogbookException">Thrown when the result is FAIL or missing.</exception>
    internal static Dictionary<string, string> CheckResult(Dictionary<string, string> map)
    {
        if (!map.TryGetValue("RESULT", out var result))
        {
            throw new QrzLogbookException("QRZ response missing RESULT field.");
        }

        if (result.Equals("OK", StringComparison.OrdinalIgnoreCase))
        {
            return map;
        }

        if (result.Equals("FAIL", StringComparison.OrdinalIgnoreCase))
        {
            var reason = map.GetValueOrDefault("REASON", "unknown error");
            if (IsAuthError(reason))
            {
                throw new QrzLogbookAuthException(reason);
            }

            throw new QrzLogbookException(reason);
        }

        throw new QrzLogbookException($"Unexpected RESULT value: {result}");
    }

    /// <summary>
    /// Extract the ADIF payload from a QRZ FETCH response.
    /// The ADIF data starts immediately after <c>ADIF=</c> and runs to the end of the body.
    /// Returns <c>null</c> if no ADIF marker is found.
    /// </summary>
    internal static string? ExtractAdifPayload(string body)
    {
        var upper = body.ToUpperInvariant();
        var marker = upper.IndexOf("ADIF=", StringComparison.Ordinal);
        if (marker < 0)
        {
            return null;
        }

        var start = marker + "ADIF=".Length;
        return start >= body.Length ? null : body[start..];
    }

    /// <summary>
    /// Parse the leading key-value portion of a FETCH response (before ADIF payload).
    /// Extracts RESULT and COUNT from the prefix before the ADIF= marker.
    /// </summary>
    internal static Dictionary<string, string> ParseFetchPrefix(string body)
    {
        var upper = body.ToUpperInvariant();
        var marker = upper.IndexOf("ADIF=", StringComparison.Ordinal);
        var prefix = marker >= 0 ? body[..marker].TrimEnd('&') : body;
        return ParseKeyValueResponse(prefix);
    }

    /// <summary>
    /// Decode HTML entities that QRZ encodes in FETCH ADIF payloads.
    /// </summary>
    internal static string DecodeHtmlEntities(string payload)
    {
        if (!payload.Contains('&', StringComparison.Ordinal))
        {
            return payload;
        }

        // Decode named/numeric entities; ampersand last to avoid double-decode.
        return payload
            .Replace("&lt;", "<", StringComparison.Ordinal)
            .Replace("&gt;", ">", StringComparison.Ordinal)
            .Replace("&quot;", "\"", StringComparison.Ordinal)
            .Replace("&#39;", "'", StringComparison.Ordinal)
            .Replace("&amp;", "&", StringComparison.Ordinal);
    }

    /// <summary>
    /// Prepend <c>&lt;EOH&gt;\n</c> if the payload does not already contain one,
    /// so the ADIF parser treats records as QSO records.
    /// </summary>
    internal static string EnsureAdifHasEoh(string payload)
    {
        if (payload.Contains("<EOH>", StringComparison.OrdinalIgnoreCase))
        {
            return payload;
        }

        return $"<EOH>\n{payload}";
    }

    /// <summary>
    /// Detects the QRZ quirk where a MODSINCE FETCH with zero matching records
    /// returns <c>RESULT=FAIL</c> with <c>COUNT=0</c> and no <c>REASON</c>.
    /// This should be treated as an empty result, not an error.
    /// </summary>
    internal static bool IsEmptyFetchFail(Dictionary<string, string> map)
    {
        return map.TryGetValue("RESULT", out var result)
            && result.Equals("FAIL", StringComparison.OrdinalIgnoreCase)
            && map.TryGetValue("COUNT", out var count)
            && count == "0"
            && !map.ContainsKey("REASON");
    }

    private static bool IsAuthError(string reason)
    {
        return reason.Contains("invalid api key", StringComparison.OrdinalIgnoreCase)
            || reason.Contains("api key required", StringComparison.OrdinalIgnoreCase)
            || reason.Contains("access denied", StringComparison.OrdinalIgnoreCase);
    }
}

/// <summary>
/// Thrown when the QRZ Logbook API returns an error.
/// </summary>
public class QrzLogbookException : Exception
{
    public QrzLogbookException() { }

    public QrzLogbookException(string message)
        : base(message) { }

    public QrzLogbookException(string message, Exception inner)
        : base(message, inner) { }
}

/// <summary>
/// Thrown when the QRZ Logbook API rejects the API key.
/// </summary>
public sealed class QrzLogbookAuthException : QrzLogbookException
{
    public QrzLogbookAuthException() { }

    public QrzLogbookAuthException(string message)
        : base(message) { }

    public QrzLogbookAuthException(string message, Exception inner)
        : base(message, inner) { }
}
