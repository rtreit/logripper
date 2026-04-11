using LogRipper.DebugHost.Models;
using LogRipper.Domain;

namespace LogRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class LookupInvocationResultTests
{
    [Fact]
    public void DebugHttpExchanges_flattens_all_response_exchanges_in_order()
    {
        var invocation = new LookupInvocationResult(
            new LookupRequest { Callsign = "W1AW" },
            [
                new LookupResult
                {
                    DebugHttpExchanges =
                    {
                        new DebugHttpExchange
                        {
                            ProviderName = "QRZ XML",
                            Operation = "login",
                            Attempt = 1
                        }
                    }
                },
                new LookupResult
                {
                    DebugHttpExchanges =
                    {
                        new DebugHttpExchange
                        {
                            ProviderName = "QRZ XML",
                            Operation = "callsign_lookup",
                            Attempt = 1
                        }
                    }
                }
            ],
            null,
            "Streaming lookup",
            DateTimeOffset.UtcNow);

        Assert.Collection(
            invocation.DebugHttpExchanges,
            exchange =>
            {
                Assert.Equal("QRZ XML", exchange.ProviderName);
                Assert.Equal("login", exchange.Operation);
            },
            exchange =>
            {
                Assert.Equal("QRZ XML", exchange.ProviderName);
                Assert.Equal("callsign_lookup", exchange.Operation);
            });
    }
}
