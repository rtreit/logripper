namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Categorizes rig control provider failures for downstream error handling.
/// </summary>
public enum RigControlErrorKind
{
    /// <summary>TCP transport error (connection refused, socket error).</summary>
    Transport,

    /// <summary>Response could not be parsed (unexpected format, RPRT error).</summary>
    Parse,

    /// <summary>Operation timed out waiting for a response.</summary>
    Timeout,

    /// <summary>Rig control is disabled by configuration.</summary>
    Disabled,
}
