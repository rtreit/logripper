using QsoRipper.EngineSelection;

namespace QsoRipper.Gui.Services;

internal sealed record EngineSwitchResult(
    bool Success,
    string Message,
    EngineTargetProfile Profile,
    string Endpoint);
