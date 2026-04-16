namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Exception thrown by rig control providers when a snapshot cannot be obtained.
/// </summary>
public sealed class RigControlException : Exception
{
    public RigControlException()
    {
    }

    public RigControlException(string message)
        : base(message)
    {
    }

    public RigControlException(string message, Exception innerException)
        : base(message, innerException)
    {
    }

    public RigControlException(string message, RigControlErrorKind kind)
        : base(message)
    {
        Kind = kind;
    }

    public RigControlException(string message, RigControlErrorKind kind, Exception innerException)
        : base(message, innerException)
    {
        Kind = kind;
    }

    /// <summary>
    /// The category of failure.
    /// </summary>
    public RigControlErrorKind Kind { get; }
}
