using QsoRipper.Engine.Lookup.Qrz;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
#pragma warning disable CA2000 // Dispose objects before losing scope — test fakes are short-lived
public sealed class QrzXmlLoginErrorTests
{
    /// <summary>
    /// When the HTTP request fails during login (e.g. DNS failure, connection refused),
    /// the lookup result should report <see cref="ProviderLookupState.NetworkError"/>,
    /// NOT <see cref="ProviderLookupState.AuthenticationError"/>.
    /// </summary>
    [Fact]
    public async Task Login_HttpRequestException_returns_NetworkError()
    {
        var provider = CreateProvider(new ThrowingHandler(new HttpRequestException("DNS resolution failed")));

        var result = await provider.LookupAsync("W1AW");

        Assert.Equal(ProviderLookupState.NetworkError, result.State);
        Assert.Contains("DNS resolution failed", result.ErrorMessage, StringComparison.Ordinal);
    }

    /// <summary>
    /// When the HTTP request times out during login (TaskCanceledException not from caller cancel),
    /// the lookup result should report <see cref="ProviderLookupState.NetworkError"/>,
    /// NOT <see cref="ProviderLookupState.AuthenticationError"/>.
    /// </summary>
    [Fact]
    public async Task Login_Timeout_returns_NetworkError()
    {
        var provider = CreateProvider(new ThrowingHandler(new TaskCanceledException("Request timed out")));

        var result = await provider.LookupAsync("W1AW");

        Assert.Equal(ProviderLookupState.NetworkError, result.State);
        Assert.Contains("timed out", result.ErrorMessage, StringComparison.OrdinalIgnoreCase);
    }

    /// <summary>
    /// When the server returns a valid XML response with no session key (auth failure),
    /// the result should still be AuthenticationError.
    /// </summary>
    [Fact]
    public async Task Login_InvalidCredentials_returns_AuthenticationError()
    {
        const string noKeyResponse = """
            <?xml version="1.0" encoding="utf-8" ?>
            <QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
                <Session>
                    <Error>Username/password incorrect</Error>
                </Session>
            </QRZDatabase>
            """;
        var provider = CreateProvider(new FixedResponseHandler(noKeyResponse));

        var result = await provider.LookupAsync("W1AW");

        Assert.Equal(ProviderLookupState.AuthenticationError, result.State);
    }

    private static QrzXmlProvider CreateProvider(HttpMessageHandler handler)
    {
        return new QrzXmlProvider(new HttpClient(handler), "testuser", "testpass");
    }

    /// <summary>Handler that always throws the given exception.</summary>
    private sealed class ThrowingHandler(Exception exception) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            return Task.FromException<HttpResponseMessage>(exception);
        }
    }

    /// <summary>Handler that always returns a fixed XML body.</summary>
    private sealed class FixedResponseHandler(string body) : HttpMessageHandler
    {
        protected override Task<HttpResponseMessage> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            return Task.FromResult(new HttpResponseMessage { Content = new StringContent(body) });
        }
    }
}
#pragma warning restore CA2000
#pragma warning restore CA1707
