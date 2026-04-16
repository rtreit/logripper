namespace QsoRipper.Engine.Lookup.Qrz;

/// <summary>
/// Returns <see cref="ProviderLookupState.NotFound"/> for every callsign.
/// Used when QRZ credentials are not configured.
/// </summary>
public sealed class DisabledCallsignProvider : ICallsignProvider
{
    public string ProviderName => "disabled";

    public Task<ProviderLookupResult> LookupAsync(string callsign, CancellationToken ct = default)
    {
        return Task.FromResult(new ProviderLookupResult
        {
            State = ProviderLookupState.NotFound,
            ErrorMessage = "QRZ XML lookup is disabled (no credentials configured).",
        });
    }
}
