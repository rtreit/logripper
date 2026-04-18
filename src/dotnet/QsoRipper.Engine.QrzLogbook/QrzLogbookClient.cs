using System.Text;
using QsoRipper.Domain;

namespace QsoRipper.Engine.QrzLogbook;

/// <summary>
/// HTTP client for the QRZ Logbook API (<c>https://logbook.qrz.com/api</c>).
/// All requests are POST with form-encoded body. Every request includes <c>KEY=&lt;api_key&gt;</c>.
/// </summary>
public sealed class QrzLogbookClient : IQrzLogbookApi, IDisposable
{
    private static readonly Uri DefaultApiUri = new("https://logbook.qrz.com/api");

    private readonly HttpClient _httpClient;
    private readonly string _apiKey;
    private readonly Uri _apiUri;
    private readonly bool _ownsHttpClient;

    /// <summary>
    /// Create a client using the provided API key.
    /// </summary>
    public QrzLogbookClient(string apiKey)
        : this(new HttpClient { Timeout = TimeSpan.FromSeconds(30) }, apiKey, DefaultApiUri, ownsHttpClient: true)
    {
    }

    /// <summary>
    /// Create a client using the provided API key and explicit QRZ API URL.
    /// </summary>
    public QrzLogbookClient(string apiKey, Uri apiUri)
        : this(new HttpClient { Timeout = TimeSpan.FromSeconds(30) }, apiKey, apiUri, ownsHttpClient: true)
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
        var formFields = new List<KeyValuePair<string, string>>(3)
        {
            new("ACTION", "FETCH"),
            new("KEY", _apiKey),
        };

        if (!string.IsNullOrWhiteSpace(sinceDateYmd))
        {
            formFields.Add(new KeyValuePair<string, string>("OPTION", $"MODSINCE:{sinceDateYmd}"));
        }

        var body = await PostFormAsync(formFields).ConfigureAwait(false);

        // Parse the prefix (RESULT, COUNT) before the ADIF payload.
        var prefix = QrzResponseParser.ParseFetchPrefix(body);
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
}
