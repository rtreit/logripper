using System.Net;
using System.Net.Http;
using QsoRipper.Engine.Lookup;
using QsoRipper.Engine.Lookup.Qrz;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class QrzXmlProviderTests
{
    [Fact]
    public async Task LookupAsync_UsesConfiguredUserAgentForLogin()
    {
        using var handler = new RecordingHandler(
            CreateXmlResponse(
                """
                <?xml version="1.0" encoding="utf-8" ?>
                <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                    <Session>
                        <Key>abc123</Key>
                    </Session>
                </QRZDatabase>
                """),
            CreateXmlResponse(
                """
                <?xml version="1.0" encoding="utf-8" ?>
                <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                    <Callsign>
                        <call>W1AW</call>
                    </Callsign>
                    <Session>
                        <Key>abc123</Key>
                    </Session>
                </QRZDatabase>
                """));
        using var httpClient = new HttpClient(handler);
        var provider = new QrzXmlProvider(httpClient, "demo-user", "demo-password", userAgent: "Managed Agent/1.0");

        var result = await provider.LookupAsync("W1AW");

        Assert.Equal(ProviderLookupState.Found, result.State);
        Assert.Equal(2, handler.RequestUris.Count);

        var loginQuery = ParseQuery(handler.RequestUris[0]);
        Assert.Equal("demo-user", loginQuery["username"]);
        Assert.Equal("demo-password", loginQuery["password"]);
        Assert.Equal("Managed Agent/1.0", loginQuery["agent"]);
    }

    private static HttpResponseMessage CreateXmlResponse(string body)
    {
        return new HttpResponseMessage(HttpStatusCode.OK)
        {
            Content = new StringContent(body),
        };
    }

    private static Dictionary<string, string> ParseQuery(Uri uri)
    {
        return uri.Query
            .TrimStart('?')
            .Split('&', StringSplitOptions.RemoveEmptyEntries)
            .Select(static entry => entry.Split('=', 2))
            .ToDictionary(
                static parts => Uri.UnescapeDataString(parts[0]),
                static parts => parts.Length == 2 ? Uri.UnescapeDataString(parts[1]) : string.Empty,
                StringComparer.Ordinal);
    }

    private sealed class RecordingHandler(params HttpResponseMessage[] responses) : HttpMessageHandler
    {
        private readonly Queue<HttpResponseMessage> _responses = new(responses);

        public List<Uri> RequestUris { get; } = [];

        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            Assert.NotNull(request.RequestUri);
            RequestUris.Add(request.RequestUri!);
            Assert.NotEmpty(_responses);
            return Task.FromResult(_responses.Dequeue());
        }
    }
}
