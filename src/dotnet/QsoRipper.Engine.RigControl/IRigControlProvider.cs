using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Reads a point-in-time snapshot of the rig's frequency and mode.
/// </summary>
/// <remarks>
/// Implementations are expected to be stateless — each call may open a fresh
/// connection. Caching and staleness tracking belong in <see cref="RigControlMonitor"/>.
/// </remarks>
public interface IRigControlProvider
{
    /// <summary>
    /// Query the rig and return a snapshot of frequency and mode.
    /// </summary>
    /// <exception cref="RigControlException">
    /// Thrown when the rig cannot be reached, the response is invalid, or the provider is disabled.
    /// </exception>
    RigSnapshot GetSnapshot();
}
