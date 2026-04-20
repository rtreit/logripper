using System.Globalization;
using QsoRipper.Domain;

namespace QsoRipper.Engine.QrzLogbook;

/// <summary>
/// HTTP client for the QRZ Logbook API (<c>https://logbook.qrz.com/api</c>).
/// All requests are POST with form-encoded body. Every request includes <c>KEY=&lt;api_key&gt;</c>.
/// </summary>
public sealed class QrzLogbookClient : IQrzLogbookApi, IDisposable
{
    private static readonly Uri DefaultApiUri = new("https://logbook.qrz.com/api");
    private const string DefaultUserAgent = "QsoRipper/0.1";

    private readonly HttpClient _httpClient;
    private readonly string _apiKey;
    private readonly Uri _apiUri;
    private readonly bool _ownsHttpClient;

    /// <summary>
    /// Create a client using the provided API key.
    /// </summary>
    public QrzLogbookClient(string apiKey)
        : this(CreateDefaultHttpClient(), apiKey, DefaultApiUri, ownsHttpClient: true)
    {
    }

    /// <summary>
    /// Create a client using the provided API key and explicit QRZ API URL.
    /// </summary>
    public QrzLogbookClient(string apiKey, Uri apiUri)
        : this(CreateDefaultHttpClient(), apiKey, apiUri, ownsHttpClient: true)
    {
    }

    /// <summary>
    /// Create a client with a caller-supplied <see cref="HttpClient"/> and optional API URL override (for testing).
    /// </summary>
    public QrzLogbookClient(HttpClient httpClient, string apiKey, Uri? apiUri = null)
        : this(httpClient, apiKey, apiUri ?? DefaultApiUri, ownsHttpClient: false)
    {
    }

    private QrzLogbookClient(HttpClient httpClient, string apiKey, Uri apiUri, bool ownsHttpClient)
    {
        ArgumentNullException.ThrowIfNull(httpClient);
        ArgumentException.ThrowIfNullOrWhiteSpace(apiKey);
        ArgumentNullException.ThrowIfNull(apiUri);

        _httpClient = httpClient;
        _apiKey = apiKey;
        _apiUri = apiUri;
        _ownsHttpClient = ownsHttpClient;
    }

    /// <inheritdoc />
    public async Task<List<QsoRecord>> FetchQsosAsync(string? sinceDateYmd)
    {
        var optionValue = string.IsNullOrWhiteSpace(sinceDateYmd)
            ? "ALL"
            : $"MODSINCE:{sinceDateYmd}";

        var formFields = new List<KeyValuePair<string, string>>(3)
        {
            new("ACTION", "FETCH"),
            new("KEY", _apiKey),
            new("OPTION", optionValue),
        };

        var body = await PostFormAsync(formFields).ConfigureAwait(false);

        // Parse the prefix (RESULT, COUNT) before the ADIF payload.
        var prefix = QrzResponseParser.ParseFetchPrefix(body);

        // QRZ returns RESULT=FAIL with COUNT=0 and no REASON for MODSINCE
        // queries that match zero records. Treat this as an empty result
        // rather than an error.
        if (QrzResponseParser.IsEmptyFetchFail(prefix))
        {
            return [];
        }

        QrzResponseParser.CheckResult(prefix);

        // Extract ADIF payload.
        var adifPayload = QrzResponseParser.ExtractAdifPayload(body);
        if (string.IsNullOrWhiteSpace(adifPayload))
        {
            return [];
        }

        // Decode HTML entities and ensure EOH marker.
        var decoded = QrzResponseParser.DecodeHtmlEntities(adifPayload);
        var normalized = QrzResponseParser.EnsureAdifHasEoh(decoded);

        return AdifCodec.ParseAdif(normalized);
    }

    /// <inheritdoc />
    public async Task<string> UploadQsoAsync(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        var adifRecord = AdifCodec.SerializeSingleQso(qso);

        var formFields = new List<KeyValuePair<string, string>>(3)
        {
            new("ACTION", "INSERT"),
            new("KEY", _apiKey),
            new("ADIF", adifRecord),
        };

        var body = await PostFormAsync(formFields).ConfigureAwait(false);
        var map = QrzResponseParser.ParseKeyValueResponse(body);
        QrzResponseParser.CheckResult(map);

        if (!map.TryGetValue("LOGID", out var logid) || string.IsNullOrWhiteSpace(logid))
        {
            throw new QrzLogbookException("INSERT response missing LOGID.");
        }

        return logid;
    }

    /// <inheritdoc />
    public async Task<string> UpdateQsoAsync(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        if (!qso.HasQrzLogid || string.IsNullOrWhiteSpace(qso.QrzLogid))
        {
            throw new QrzLogbookException("REPLACE requires a QRZ LOGID but the QSO has none.");
        }

        var adifRecord = AdifCodec.SerializeSingleQso(qso);

        // Per docs/integrations/qrz-logbook-api.md the documented way to
        // update an existing QSO is ACTION=INSERT with
        // OPTION=REPLACE,LOGID:<id>. The API returns RESULT=REPLACE.
        var formFields = new List<KeyValuePair<string, string>>(4)
        {
            new("ACTION", "INSERT"),
            new("OPTION", $"REPLACE,LOGID:{qso.QrzLogid}"),
            new("KEY", _apiKey),
            new("ADIF", adifRecord),
        };

        var body = await PostFormAsync(formFields).ConfigureAwait(false);
        var map = QrzResponseParser.ParseKeyValueResponse(body);
        QrzResponseParser.CheckResult(map);

        // QRZ echoes the same LOGID on REPLACE; if absent, fall back.
        if (map.TryGetValue("LOGID", out var logid) && !string.IsNullOrWhiteSpace(logid))
        {
            return logid;
        }

        return qso.QrzLogid;
    }

    /// <inheritdoc />
    public async Task<QrzLogbookStatus> GetStatusAsync()
    {
        var formFields = new List<KeyValuePair<string, string>>(2)
        {
            new("ACTION", "STATUS"),
            new("KEY", _apiKey),
        };

        var body = await PostFormAsync(formFields).ConfigureAwait(false);
        var map = QrzResponseParser.ParseKeyValueResponse(body);
        QrzResponseParser.CheckResult(map);

        // Match Rust qsoripper-core/src/qrz_logbook/mod.rs::test_connection:
        // prefer CALLSIGN, fall back to OWNER. COUNT may be missing on a brand-new
        // logbook; treat that as zero rather than failing.
        var owner = map.TryGetValue("CALLSIGN", out var callsign) && !string.IsNullOrWhiteSpace(callsign)
            ? callsign
            : map.GetValueOrDefault("OWNER", string.Empty);

        uint qsoCount = 0;
        if (map.TryGetValue("COUNT", out var countText)
            && uint.TryParse(countText, NumberStyles.Integer, CultureInfo.InvariantCulture, out var parsed))
        {
            qsoCount = parsed;
        }

        return new QrzLogbookStatus(owner, qsoCount);
    }

    /// <inheritdoc />
    public async Task DeleteQsoAsync(string logid)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(logid);

        var formFields = new List<KeyValuePair<string, string>>(3)
        {
            new("ACTION", "DELETE"),
            new("KEY", _apiKey),
            new("LOGID", logid),
        };

        string body;
        try
        {
            body = await PostFormAsync(formFields).ConfigureAwait(false);
        }
        catch (QrzLogbookException ex) when (ex.Message.StartsWith("HTTP 404", StringComparison.Ordinal))
        {
            // Already gone server-side — treat as success.
            return;
        }

        var map = QrzResponseParser.ParseKeyValueResponse(body);
        try
        {
            QrzResponseParser.CheckResult(map);
        }
        catch (QrzLogbookAuthException)
        {
            throw;
        }
        catch (QrzLogbookException ex) when (QrzResponseParser.IsNotFoundError(ex.Message))
        {
            // QRZ says the record is already gone — treat as success so the
            // queued-remote-delete loop can clear local pending flags and
            // doesn't loop forever.
            return;
        }
    }

    /// <inheritdoc cref="IDisposable.Dispose"/>
    public void Dispose()
    {
        if (_ownsHttpClient)
        {
            _httpClient.Dispose();
        }
    }

    private async Task<string> PostFormAsync(List<KeyValuePair<string, string>> fields)
    {
        using var content = new FormUrlEncodedContent(fields);

        using var response = await _httpClient.PostAsync(_apiUri, content).ConfigureAwait(false);

        var body = await response.Content.ReadAsStringAsync().ConfigureAwait(false);

        if (!response.IsSuccessStatusCode)
        {
            throw new QrzLogbookException(
                $"HTTP {(int)response.StatusCode}: {Truncate(body, 200)}");
        }

        return body;
    }

    private static string Truncate(string value, int maxLength) =>
        value.Length <= maxLength ? value : string.Concat(value.AsSpan(0, maxLength), "…");

    private static HttpClient CreateDefaultHttpClient()
    {
        var client = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };
        client.DefaultRequestHeaders.UserAgent.ParseAdd(DefaultUserAgent);
        return client;
    }
}
