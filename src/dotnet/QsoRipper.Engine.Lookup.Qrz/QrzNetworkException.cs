namespace QsoRipper.Engine.Lookup.Qrz;

/// <summary>
/// Thrown when a QRZ XML API login attempt fails due to a network-level error
/// (DNS failure, connection refused, timeout) rather than an authentication error.
/// </summary>
public sealed class QrzNetworkException : Exception
{
    public QrzNetworkException()
    {
    }

    public QrzNetworkException(string message)
        : base(message)
    {
    }

    public QrzNetworkException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
