using QsoRipper.Domain;

namespace QsoRipper.DebugHost.Models;

internal sealed class QsoSampleOptions
{
    public Band Band { get; set; } = Band._20M;

    public Mode Mode { get; set; } = Mode.Ssb;
}
