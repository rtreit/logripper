namespace QsoRipper.Cli;

internal static class CliEndpointValidator
{
    public static bool TryCreateEndpointUri(string endpoint, out Uri? uri)
    {
        if (!Uri.TryCreate(endpoint, UriKind.Absolute, out uri))
        {
            return false;
        }

        return string.Equals(uri.Scheme, Uri.UriSchemeHttp, StringComparison.OrdinalIgnoreCase)
            || string.Equals(uri.Scheme, Uri.UriSchemeHttps, StringComparison.OrdinalIgnoreCase);
    }
}
