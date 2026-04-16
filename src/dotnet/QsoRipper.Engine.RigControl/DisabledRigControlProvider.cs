using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl;

/// <summary>
/// Placeholder provider used when rig control is not configured.
/// Always throws <see cref="RigControlException"/> with <see cref="RigControlErrorKind.Disabled"/>.
/// </summary>
public sealed class DisabledRigControlProvider : IRigControlProvider
{
    public RigSnapshot GetSnapshot()
    {
        throw new RigControlException("Rig control is disabled.", RigControlErrorKind.Disabled);
    }
}
