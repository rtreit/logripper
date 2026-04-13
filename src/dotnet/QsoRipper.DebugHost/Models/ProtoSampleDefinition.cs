namespace QsoRipper.DebugHost.Models;

internal sealed record ProtoSampleDefinition(
    string Id,
    string Label,
    Type MessageType,
    bool SupportsQsoOptions);
