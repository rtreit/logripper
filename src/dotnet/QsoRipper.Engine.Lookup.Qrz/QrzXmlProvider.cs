using System.Globalization;
using System.Xml.Linq;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;

namespace QsoRipper.Engine.Lookup.Qrz;

/// <summary>
/// Callsign provider that queries the QRZ XML API.
/// </summary>
public sealed class QrzXmlProvider : ICallsignProvider
{
    private const string DefaultBaseUrl = "https://xmldata.qrz.com/xml/current/";
    private const string DefaultAgentName = "qsoripper-dotnet";

    private readonly HttpClient _httpClient;
    private readonly string _username;
    private readonly string _password;
    private readonly string _baseUrl;
    private readonly string _userAgent;
    private readonly Lock _sessionLock = new();
    private string? _sessionKey;

    public string ProviderName => "QRZ XML";

    /// <summary>Create a new QRZ XML provider.</summary>
    [System.Diagnostics.CodeAnalysis.SuppressMessage("Design", "CA1054:URI parameters should not be strings", Justification = "URL is built from env config, not user-facing API")]
    public QrzXmlProvider(HttpClient httpClient, string username, string password, string? baseUrl = null, string? userAgent = null)
    {
        ArgumentNullException.ThrowIfNull(httpClient);
        ArgumentException.ThrowIfNullOrWhiteSpace(username);
        ArgumentException.ThrowIfNullOrWhiteSpace(password);

        _httpClient = httpClient;
        _username = username;
        _password = password;
        _baseUrl = NormalizeBaseUrl(baseUrl ?? DefaultBaseUrl);
        _userAgent = NormalizeUserAgent(userAgent);
    }

    /// <inheritdoc/>
    public async Task<ProviderLookupResult> LookupAsync(string callsign, CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(callsign);
        var normalized = callsign.Trim().ToUpperInvariant();

        // Ensure we have a session key (login if needed).
        string? sessionKey;
        try
        {
            sessionKey = await EnsureSessionKeyAsync(ct).ConfigureAwait(false);
        }
        catch (QrzNetworkException ex)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.NetworkError,
                ErrorMessage = $"Network error during QRZ login: {ex.Message}",
            };
        }

        if (sessionKey is null)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.AuthenticationError,
                ErrorMessage = "Failed to obtain QRZ session key.",
            };
        }

        // First attempt
        var result = await DoLookupAsync(sessionKey, normalized, ct).ConfigureAwait(false);
        if (result is not null)
        {
            return result;
        }

        // Session expired — re-login and retry once
        ClearSessionKey();
        try
        {
            sessionKey = await LoginAsync(ct).ConfigureAwait(false);
        }
        catch (QrzNetworkException ex)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.NetworkError,
                ErrorMessage = $"Network error during QRZ re-login: {ex.Message}",
            };
        }

        if (sessionKey is null)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.AuthenticationError,
                ErrorMessage = "Failed to re-authenticate with QRZ after session expiry.",
            };
        }

        return await DoLookupAsync(sessionKey, normalized, ct).ConfigureAwait(false)
            ?? new ProviderLookupResult
            {
                State = ProviderLookupState.SessionError,
                ErrorMessage = "QRZ session invalid after re-login.",
            };
    }

    /// <summary>
    /// Perform a lookup using the given session key.
    /// Returns null if the session is expired/invalid (caller should re-login).
    /// </summary>
    private async Task<ProviderLookupResult?> DoLookupAsync(string sessionKey, string callsign, CancellationToken ct)
    {
        var url = new Uri($"{_baseUrl}?s={Uri.EscapeDataString(sessionKey)}&callsign={Uri.EscapeDataString(callsign)}");

        string body;
        try
        {
            body = await _httpClient.GetStringAsync(url, ct).ConfigureAwait(false);
        }
        catch (HttpRequestException ex)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.NetworkError,
                ErrorMessage = $"HTTP request failed: {ex.Message}",
            };
        }
        catch (TaskCanceledException ex) when (!ct.IsCancellationRequested)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.NetworkError,
                ErrorMessage = $"HTTP request timed out: {ex.Message}",
            };
        }

        var doc = ParseXml(body);
        if (doc is null)
        {
            return new ProviderLookupResult
            {
                State = ProviderLookupState.NetworkError,
                ErrorMessage = "Failed to parse QRZ XML response.",
            };
        }

        // Check for session errors
        var ns = doc.Root?.Name.Namespace ?? XNamespace.None;
        var sessionElement = doc.Root?.Element(ns + "Session");
        var sessionError = sessionElement?.Element(ns + "Error")?.Value;
        if (sessionError is not null)
        {
            if (IsNotFoundError(sessionError))
            {
                return new ProviderLookupResult { State = ProviderLookupState.NotFound };
            }

            if (IsSessionExpiredError(sessionError))
            {
                return null; // Signal re-login
            }

            if (IsAuthError(sessionError))
            {
                return new ProviderLookupResult
                {
                    State = ProviderLookupState.AuthenticationError,
                    ErrorMessage = $"QRZ authentication error: {sessionError}",
                };
            }

            return new ProviderLookupResult
            {
                State = ProviderLookupState.SessionError,
                ErrorMessage = $"QRZ session error: {sessionError}",
            };
        }

        // Parse callsign element
        var callsignElement = doc.Root?.Element(ns + "Callsign");
        if (callsignElement is null)
        {
            return new ProviderLookupResult { State = ProviderLookupState.NotFound };
        }

        var record = MapCallsignRecord(callsign, callsignElement);
        DxccEnrichment.Enrich(record);

        return new ProviderLookupResult
        {
            State = ProviderLookupState.Found,
            Record = record,
        };
    }

    private async Task<string?> EnsureSessionKeyAsync(CancellationToken ct)
    {
        string? existing;
        lock (_sessionLock)
        {
            existing = _sessionKey;
        }

        if (existing is not null)
        {
            return existing;
        }

        return await LoginAsync(ct).ConfigureAwait(false);
    }

    /// <summary>
    /// Attempts to log in to the QRZ XML API.
    /// Returns null if the server responded but no session key was found (auth failure).
    /// Throws <see cref="QrzNetworkException"/> if the request could not reach the server.
    /// </summary>
    private async Task<string?> LoginAsync(CancellationToken ct)
    {
        var url = new Uri($"{_baseUrl}?username={Uri.EscapeDataString(_username)}&password={Uri.EscapeDataString(_password)}&agent={Uri.EscapeDataString(_userAgent)}");

        string body;
        try
        {
            body = await _httpClient.GetStringAsync(url, ct).ConfigureAwait(false);
        }
        catch (HttpRequestException ex)
        {
            throw new QrzNetworkException($"HTTP request failed: {ex.Message}", ex);
        }
        catch (TaskCanceledException ex) when (!ct.IsCancellationRequested)
        {
            throw new QrzNetworkException($"HTTP request timed out: {ex.Message}", ex);
        }

        var doc = ParseXml(body);
        var ns = doc?.Root?.Name.Namespace ?? XNamespace.None;
        var key = doc?.Root?.Element(ns + "Session")?.Element(ns + "Key")?.Value;

        if (string.IsNullOrWhiteSpace(key))
        {
            return null;
        }

        lock (_sessionLock)
        {
            _sessionKey = key;
        }

        return key;
    }

    private void ClearSessionKey()
    {
        lock (_sessionLock)
        {
            _sessionKey = null;
        }
    }

    internal static CallsignRecord MapCallsignRecord(string queriedCallsign, XElement callsignElement)
    {
        var ns = callsignElement.Name.Namespace;
        string Str(string name) => callsignElement.Element(ns + name)?.Value?.Trim() ?? string.Empty;
        string? OptStr(string name)
        {
            var val = callsignElement.Element(ns + name)?.Value?.Trim();
            return string.IsNullOrEmpty(val) ? null : val;
        }

        var callsign = OptStr("call") ?? queriedCallsign.Trim().ToUpperInvariant();
        var crossRef = OptStr("xref") ?? queriedCallsign.Trim().ToUpperInvariant();

        var record = new CallsignRecord
        {
            Callsign = callsign.Trim().ToUpperInvariant(),
            CrossRef = crossRef.Trim().ToUpperInvariant(),
            PreviousCall = Str("p_call"),
            DxccEntityId = ParseUint(OptStr("dxcc")),
            FirstName = Str("fname"),
            LastName = Str("name"),
            GeoSource = MapGeoSource(OptStr("geoloc")),
            Eqsl = MapQslPreference(OptStr("eqsl")),
            Lotw = MapQslPreference(OptStr("lotw")),
            PaperQsl = MapQslPreference(OptStr("mqsl")),
        };

        // Aliases (comma-separated)
        var aliases = OptStr("aliases");
        if (aliases is not null)
        {
            foreach (var alias in aliases.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries))
            {
                if (!string.IsNullOrEmpty(alias))
                {
                    record.Aliases.Add(alias);
                }
            }
        }

        // Optional strings
        SetOptional(record, OptStr("nickname"), static (r, v) => r.Nickname = v);
        SetOptional(record, OptStr("name_fmt"), static (r, v) => r.FormattedName = v);
        SetOptional(record, OptStr("attn"), static (r, v) => r.Attention = v);
        SetOptional(record, OptStr("addr1"), static (r, v) => r.Addr1 = v);
        SetOptional(record, OptStr("addr2"), static (r, v) => r.Addr2 = v);
        SetOptional(record, OptStr("state"), static (r, v) => r.State = v);
        SetOptional(record, OptStr("zip"), static (r, v) => r.Zip = v);
        SetOptional(record, OptStr("country"), static (r, v) => r.Country = v);
        SetOptionalUint(record, OptStr("ccode"), static (r, v) => r.CountryCode = v);
        SetOptionalDouble(record, OptStr("lat"), static (r, v) => r.Latitude = v);
        SetOptionalDouble(record, OptStr("lon"), static (r, v) => r.Longitude = v);
        SetOptional(record, OptStr("grid"), static (r, v) => r.GridSquare = v);
        SetOptional(record, OptStr("county"), static (r, v) => r.County = v);
        SetOptional(record, OptStr("fips"), static (r, v) => r.Fips = v);
        SetOptional(record, OptStr("class"), static (r, v) => r.LicenseClass = v);
        SetOptionalTimestamp(record, OptStr("efdate"), static (r, v) => r.EffectiveDate = v);
        SetOptionalTimestamp(record, OptStr("expdate"), static (r, v) => r.ExpirationDate = v);
        SetOptional(record, OptStr("codes"), static (r, v) => r.LicenseCodes = v);
        SetOptional(record, OptStr("email"), static (r, v) => r.Email = v);
        SetOptional(record, OptStr("url"), static (r, v) => r.WebUrl = v);
        SetOptional(record, OptStr("qslmgr"), static (r, v) => r.QslManager = v);
        SetOptionalUint(record, OptStr("cqzone"), static (r, v) => r.CqZone = v);
        SetOptionalUint(record, OptStr("ituzone"), static (r, v) => r.ItuZone = v);
        SetOptional(record, OptStr("iota"), static (r, v) => r.Iota = v);
        SetOptional(record, OptStr("land"), static (r, v) => r.DxccCountryName = v);
        SetOptional(record, OptStr("continent"), static (r, v) => r.DxccContinent = v);
        SetOptionalUint(record, OptStr("born"), static (r, v) => r.BirthYear = v);
        SetOptionalUlong(record, OptStr("serial"), static (r, v) => r.QrzSerial = v);
        SetOptionalDateTimeTimestamp(record, OptStr("moddate"), static (r, v) => r.LastModified = v);
        SetOptionalBioLength(record, OptStr("bio"), static (r, v) => r.BioLength = v);
        SetOptional(record, OptStr("image"), static (r, v) => r.ImageUrl = v);
        SetOptional(record, OptStr("MSA"), static (r, v) => r.Msa = v);
        SetOptional(record, OptStr("AreaCode"), static (r, v) => r.AreaCode = v);
        SetOptional(record, OptStr("TimeZone"), static (r, v) => r.TimeZone = v);
        SetOptionalGmtOffset(record, OptStr("GMTOffset"), static (r, v) => r.GmtOffset = v);
        SetOptionalBool(record, OptStr("DST"), static (r, v) => r.DstObserved = v);
        SetOptionalUint(record, OptStr("u_views"), static (r, v) => r.ProfileViews = v);

        return record;
    }

    private static void SetOptional(CallsignRecord record, string? value, Action<CallsignRecord, string> setter)
    {
        if (value is not null)
        {
            setter(record, value);
        }
    }

    private static void SetOptionalUint(CallsignRecord record, string? value, Action<CallsignRecord, uint> setter)
    {
        if (value is not null && uint.TryParse(value, CultureInfo.InvariantCulture, out var parsed))
        {
            setter(record, parsed);
        }
    }

    private static void SetOptionalUlong(CallsignRecord record, string? value, Action<CallsignRecord, ulong> setter)
    {
        if (value is not null && ulong.TryParse(value, CultureInfo.InvariantCulture, out var parsed))
        {
            setter(record, parsed);
        }
    }

    private static void SetOptionalDouble(CallsignRecord record, string? value, Action<CallsignRecord, double> setter)
    {
        if (value is not null && double.TryParse(value, CultureInfo.InvariantCulture, out var parsed))
        {
            setter(record, parsed);
        }
    }

    private static void SetOptionalBool(CallsignRecord record, string? value, Action<CallsignRecord, bool> setter)
    {
        if (value is null)
        {
            return;
        }

        var upper = value.Trim().ToUpperInvariant();
        if (upper is "Y" or "YES" or "1")
        {
            setter(record, true);
        }
        else if (upper is "N" or "NO" or "0")
        {
            setter(record, false);
        }
    }

    private static void SetOptionalTimestamp(CallsignRecord record, string? value, Action<CallsignRecord, Timestamp> setter)
    {
        if (value is null)
        {
            return;
        }

        if (DateTimeOffset.TryParseExact(value, "yyyy-MM-dd", CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var dto))
        {
            setter(record, Timestamp.FromDateTimeOffset(dto));
        }
    }

    private static void SetOptionalDateTimeTimestamp(CallsignRecord record, string? value, Action<CallsignRecord, Timestamp> setter)
    {
        if (value is null)
        {
            return;
        }

        if (DateTimeOffset.TryParseExact(value, "yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var dto))
        {
            setter(record, Timestamp.FromDateTimeOffset(dto));
        }
        else if (DateTimeOffset.TryParseExact(value, "yyyy-MM-dd", CultureInfo.InvariantCulture, DateTimeStyles.AssumeUniversal, out var dto2))
        {
            setter(record, Timestamp.FromDateTimeOffset(dto2));
        }
    }

    private static void SetOptionalBioLength(CallsignRecord record, string? value, Action<CallsignRecord, uint> setter)
    {
        if (value is null)
        {
            return;
        }

        // Bio may be "123" or "123/456" — take the first part
        var sizePart = value.Split('/')[0];
        if (uint.TryParse(sizePart.Trim(), CultureInfo.InvariantCulture, out var parsed))
        {
            setter(record, parsed);
        }
    }

    private static void SetOptionalGmtOffset(CallsignRecord record, string? value, Action<CallsignRecord, double> setter)
    {
        if (value is null)
        {
            return;
        }

        var raw = value.Trim();
        if (string.IsNullOrEmpty(raw))
        {
            return;
        }

        // Handle HHMM format like "-0800"
        var sign = 1.0;
        var digits = raw;
        if (raw.StartsWith('-'))
        {
            sign = -1.0;
            digits = raw[1..];
        }
        else if (raw.StartsWith('+'))
        {
            digits = raw[1..];
        }

        if (digits.Length == 4 && int.TryParse(digits[..2], CultureInfo.InvariantCulture, out var hours)
            && int.TryParse(digits[2..], CultureInfo.InvariantCulture, out var minutes))
        {
            setter(record, sign * (hours + minutes / 60.0));
            return;
        }

        if (double.TryParse(raw, CultureInfo.InvariantCulture, out var parsed))
        {
            setter(record, parsed);
        }
    }

    internal static GeoSource MapGeoSource(string? value)
    {
        var trimmed = value?.Trim();
        if (string.IsNullOrEmpty(trimmed))
        {
            return GeoSource.Unspecified;
        }

        if (trimmed.Equals("user", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.User;
        }

        if (trimmed.Equals("geocode", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.Geocode;
        }

        if (trimmed.Equals("grid", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.Grid;
        }

        if (trimmed.Equals("zip", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.Zip;
        }

        if (trimmed.Equals("state", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.State;
        }

        if (trimmed.Equals("dxcc", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.Dxcc;
        }

        if (trimmed.Equals("none", StringComparison.OrdinalIgnoreCase))
        {
            return GeoSource.None;
        }

        return GeoSource.Unspecified;
    }

    internal static QslPreference MapQslPreference(string? value)
    {
        return (value?.Trim().ToUpperInvariant()) switch
        {
            "1" or "Y" => QslPreference.Yes,
            "0" or "N" => QslPreference.No,
            _ => QslPreference.Unknown,
        };
    }

    private static uint ParseUint(string? value)
    {
        if (value is not null && uint.TryParse(value, CultureInfo.InvariantCulture, out var result))
        {
            return result;
        }

        return 0;
    }

    private static XDocument? ParseXml(string body)
    {
        try
        {
            return XDocument.Parse(body);
        }
        catch (System.Xml.XmlException)
        {
            return null;
        }
    }

    private static bool IsNotFoundError(string error)
    {
        return error.Contains("not found", StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsSessionExpiredError(string error)
    {
        return error.Contains("session timeout", StringComparison.OrdinalIgnoreCase)
            || error.Contains("invalid session", StringComparison.OrdinalIgnoreCase);
    }

    private static bool IsAuthError(string error)
    {
        return error.Contains("password", StringComparison.OrdinalIgnoreCase)
            || error.Contains("username", StringComparison.OrdinalIgnoreCase)
            || error.Contains("login", StringComparison.OrdinalIgnoreCase)
            || error.Contains("authorization", StringComparison.OrdinalIgnoreCase);
    }

    private static string NormalizeBaseUrl(string baseUrl)
    {
        return baseUrl.EndsWith('/') ? baseUrl : baseUrl + '/';
    }

    private static string NormalizeUserAgent(string? userAgent)
    {
        return string.IsNullOrWhiteSpace(userAgent)
            ? DefaultAgentName
            : userAgent.Trim();
    }
}
