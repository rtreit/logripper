using LogRipper.Domain;

namespace LogRipper.DebugHost.Models;

public sealed class QsoSampleOptions
{
    public Band Band { get; set; } = Band._20M;

    public Mode Mode { get; set; } = Mode.Ssb;
}
