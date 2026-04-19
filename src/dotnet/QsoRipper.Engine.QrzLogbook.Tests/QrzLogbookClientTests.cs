using System.Net;
using QsoRipper.Engine.QrzLogbook;

#pragma warning disable CA1307 // Use StringComparison for string comparison

namespace QsoRipper.Engine.QrzLogbook.Tests;

#pragma warning disable CA1707 // Remove underscores from member names

public sealed class QrzLogbookClientTests
{
    private const string FakeApiKey = "test-api-key";

    // -- FETCH OPTION field --------------------------------------------------

    [Fact]
    public async Task Fetch_without_since_date_sends_option_all()
    {
        var (client, handler) = CreateClient("RESULT=OK&COUNT=0");

        using (client)
        {
            await client.FetchQsosAsync(sinceDateYmd: null);
        }

        Assert.Contains("OPTION=ALL", handler.CapturedBody);
    }

    [Fact]
    public async Task Fetch_with_empty_since_date_sends_option_all()
    {
        var (client, handler) = CreateClient("RESULT=OK&COUNT=0");

        using (client)
        {
            await client.FetchQsosAsync(sinceDateYmd: "");
        }

        Assert.Contains("OPTION=ALL", handler.CapturedBody);
    }

    [Fact]
    public async Task Fetch_with_since_date_sends_modsince()
    {
        var (client, handler) = CreateClient("RESULT=OK&COUNT=0");

        using (client)
        {
            await client.FetchQsosAsync(sinceDateYmd: "2024-06-15");
        }

        Assert.Contains("OPTION=MODSINCE", handler.CapturedBody);
        Assert.Contains("2024-06-15", handler.CapturedBody);
    }

    // -- Error handling ------------------------------------------------------

    [Fact]
    public async Task Fetch_fail_without_reason_throws_with_unknown_error()
    {
        var (client, _) = CreateClient("RESULT=FAIL");

        using (client)
        {
            var ex = await Assert.ThrowsAsync<QrzLogbookException>(
                () => client.FetchQsosAsync(sinceDateYmd: null));

            Assert.Equal("unknown error", ex.Message);
        }
    }

    [Fact]
    public async Task Fetch_fail_with_reason_throws_with_reason()
    {
        var (client, _) = CreateClient("RESULT=FAIL&REASON=rate+limited");

        using (client)
        {
            var ex = await Assert.ThrowsAsync<QrzLogbookException>(
                () => client.FetchQsosAsync(sinceDateYmd: null));

            Assert.Equal("rate+limited", ex.Message);
        }
    }

    // -- MODSINCE empty-result quirk -----------------------------------------

    [Fact]
    public async Task Fetch_modsince_fail_count0_no_reason_returns_empty()
    {
        // QRZ returns RESULT=FAIL with COUNT=0 and no REASON for MODSINCE
        // queries that match zero records. This should be treated as empty.
        var (client, _) = CreateClient("COUNT=0&RESULT=FAIL");

        using (client)
        {
            var result = await client.FetchQsosAsync(sinceDateYmd: "2026-04-19");
            Assert.Empty(result);
        }
    }

    [Fact]
    public async Task Fetch_modsince_fail_count0_with_reason_throws()
    {
        // If REASON is present, it's a real error — do NOT swallow it.
        var (client, _) = CreateClient("COUNT=0&RESULT=FAIL&REASON=bad+key");

        using (client)
        {
            var ex = await Assert.ThrowsAsync<QrzLogbookException>(
                () => client.FetchQsosAsync(sinceDateYmd: "2026-04-19"));

            Assert.Equal("bad+key", ex.Message);
        }
    }

    [Fact]
    public async Task Fetch_result_fail_count0_no_reason_returns_empty_regardless_of_field_order()
    {
        // Verify the fix works even when RESULT comes before COUNT.
        var (client, _) = CreateClient("RESULT=FAIL&COUNT=0");

        using (client)
        {
            var result = await client.FetchQsosAsync(sinceDateYmd: "2026-04-19");
            Assert.Empty(result);
        }
    }

    // -- Helpers --------------------------------------------------------------

    private static (QrzLogbookClient Client, CapturingHandler Handler) CreateClient(string responseBody)
    {
        var handler = new CapturingHandler(responseBody);
#pragma warning disable CA2000 // httpClient lifetime managed by QrzLogbookClient
        var httpClient = new HttpClient(handler) { Timeout = TimeSpan.FromSeconds(5) };
        var client = new QrzLogbookClient(httpClient, FakeApiKey, new Uri("http://localhost/api"));
#pragma warning restore CA2000
        return (client, handler);
    }

    private sealed class CapturingHandler : DelegatingHandler
    {
        private readonly string _responseBody;

        public string? CapturedBody { get; private set; }

        public CapturingHandler(string responseBody)
        {
            _responseBody = responseBody;
            InnerHandler = new HttpClientHandler();
        }

        protected override async Task<HttpResponseMessage> SendAsync(
            HttpRequestMessage request,
            CancellationToken cancellationToken)
        {
            if (request.Content is not null)
            {
                CapturedBody = await request.Content.ReadAsStringAsync(cancellationToken);
            }

            return new HttpResponseMessage(HttpStatusCode.OK)
            {
                Content = new StringContent(_responseBody),
            };
        }
    }
}
