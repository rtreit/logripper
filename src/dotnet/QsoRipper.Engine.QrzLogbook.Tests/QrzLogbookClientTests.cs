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

    // -- UpdateQso (REPLACE) -------------------------------------------------

    [Fact]
    public async Task Update_sends_action_insert_with_option_replace_logid()
    {
        // QRZ logbook docs specify ACTION=INSERT&OPTION=REPLACE,LOGID:<id>
        // for updating an existing QSO. Using the undocumented ACTION=REPLACE
        // can cause duplicate inserts on some API endpoints.
        var (client, handler) = CreateClient("RESULT=REPLACE&LOGID=42");

        using (client)
        {
            var qso = new QsoRipper.Domain.QsoRecord
            {
                LocalId = "00000000-0000-0000-0000-000000000001",
                WorkedCallsign = "W1AW",
                Band = QsoRipper.Domain.Band._20M,
                Mode = QsoRipper.Domain.Mode.Ft8,
                UtcTimestamp = Google.Protobuf.WellKnownTypes.Timestamp.FromDateTimeOffset(
                    new DateTimeOffset(2024, 6, 15, 12, 0, 0, TimeSpan.Zero)),
                QrzLogid = "42",
            };

            var returned = await client.UpdateQsoAsync(qso);

            Assert.Equal("42", returned);
        }

        Assert.NotNull(handler.CapturedBody);
        Assert.Contains("ACTION=INSERT", handler.CapturedBody!);
        Assert.Contains("OPTION=REPLACE%2CLOGID%3A42", handler.CapturedBody!);
    }

    [Fact]
    public async Task Update_accepts_response_with_result_replace()
    {
        // The REPLACE action returns RESULT=REPLACE (not RESULT=OK). Parser
        // must treat that as success.
        var (client, _) = CreateClient("RESULT=REPLACE&LOGID=99");

        using (client)
        {
            var qso = new QsoRipper.Domain.QsoRecord
            {
                LocalId = "00000000-0000-0000-0000-000000000002",
                WorkedCallsign = "K5ABC",
                Band = QsoRipper.Domain.Band._40M,
                Mode = QsoRipper.Domain.Mode.Cw,
                UtcTimestamp = Google.Protobuf.WellKnownTypes.Timestamp.FromDateTimeOffset(
                    new DateTimeOffset(2024, 6, 15, 12, 0, 0, TimeSpan.Zero)),
                QrzLogid = "99",
            };

            var returned = await client.UpdateQsoAsync(qso);
            Assert.Equal("99", returned);
        }
    }

    [Fact]
    public async Task Update_falls_back_to_supplied_logid_when_response_omits_it()
    {
        var (client, _) = CreateClient("RESULT=REPLACE");

        using (client)
        {
            var qso = new QsoRipper.Domain.QsoRecord
            {
                LocalId = "00000000-0000-0000-0000-000000000003",
                WorkedCallsign = "N0ABC",
                Band = QsoRipper.Domain.Band._20M,
                Mode = QsoRipper.Domain.Mode.Ft8,
                UtcTimestamp = Google.Protobuf.WellKnownTypes.Timestamp.FromDateTimeOffset(
                    new DateTimeOffset(2024, 6, 15, 12, 0, 0, TimeSpan.Zero)),
                QrzLogid = "777",
            };

            var returned = await client.UpdateQsoAsync(qso);
            Assert.Equal("777", returned);
        }
    }

    // -- Helpers --------------------------------------------------------------

    // -- STATUS --------------------------------------------------------------

    [Fact]
    public async Task GetStatus_sends_action_status_with_key()
    {
        var (client, handler) = CreateClient("RESULT=OK&CALLSIGN=KC7AVA&COUNT=500");

        using (client)
        {
            await client.GetStatusAsync();
        }

        Assert.NotNull(handler.CapturedBody);
        Assert.Contains("ACTION=STATUS", handler.CapturedBody!);
        Assert.Contains($"KEY={FakeApiKey}", handler.CapturedBody!);
    }

    [Fact]
    public async Task GetStatus_parses_callsign_and_count()
    {
        var (client, _) = CreateClient("RESULT=OK&CALLSIGN=KC7AVA&COUNT=500");

        using (client)
        {
            var status = await client.GetStatusAsync();

            Assert.Equal("KC7AVA", status.Owner);
            Assert.Equal(500u, status.QsoCount);
        }
    }

    [Fact]
    public async Task GetStatus_falls_back_to_owner_field_when_callsign_missing()
    {
        // Some QRZ responses historically used OWNER instead of CALLSIGN.
        var (client, _) = CreateClient("RESULT=OK&OWNER=W1AW&COUNT=42");

        using (client)
        {
            var status = await client.GetStatusAsync();

            Assert.Equal("W1AW", status.Owner);
            Assert.Equal(42u, status.QsoCount);
        }
    }

    [Fact]
    public async Task GetStatus_defaults_count_to_zero_when_missing()
    {
        // A brand-new empty logbook may not include COUNT at all.
        var (client, _) = CreateClient("RESULT=OK&CALLSIGN=N0CALL");

        using (client)
        {
            var status = await client.GetStatusAsync();

            Assert.Equal("N0CALL", status.Owner);
            Assert.Equal(0u, status.QsoCount);
        }
    }

    [Fact]
    public async Task GetStatus_throws_auth_exception_on_invalid_api_key()
    {
        var (client, _) = CreateClient("RESULT=FAIL&REASON=invalid api key");

        using (client)
        {
            await Assert.ThrowsAsync<QrzLogbookAuthException>(() => client.GetStatusAsync());
        }
    }

    // -- DELETE --------------------------------------------------------------

    [Fact]
    public async Task Delete_sends_action_delete_with_logid()
    {
        var (client, handler) = CreateClient("RESULT=OK");

        using (client)
        {
            await client.DeleteQsoAsync("123456");
        }

        Assert.Contains("ACTION=DELETE", handler.CapturedBody);
        Assert.Contains("LOGID=123456", handler.CapturedBody);
    }

    [Fact]
    public async Task Delete_treats_not_found_reason_as_success()
    {
        var (client, _) = CreateClient("RESULT=FAIL&REASON=record not found");

        using (client)
        {
            // Should NOT throw — queued-remote-delete loop relies on this.
            await client.DeleteQsoAsync("000000");
        }
    }

    [Fact]
    public async Task Delete_propagates_other_failure_reasons()
    {
        var (client, _) = CreateClient("RESULT=FAIL&REASON=bad record format");

        using (client)
        {
            var ex = await Assert.ThrowsAsync<QrzLogbookException>(() => client.DeleteQsoAsync("000000"));
            Assert.Contains("bad record format", ex.Message, StringComparison.OrdinalIgnoreCase);
        }
    }

    [Fact]
    public async Task Delete_propagates_auth_failure()
    {
        var (client, _) = CreateClient("RESULT=FAIL&REASON=invalid api key");

        using (client)
        {
            await Assert.ThrowsAsync<QrzLogbookAuthException>(() => client.DeleteQsoAsync("000000"));
        }
    }

    // -- Client helper --------------------------------------------------------

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
