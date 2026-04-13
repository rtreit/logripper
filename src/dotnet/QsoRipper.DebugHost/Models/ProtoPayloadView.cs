namespace QsoRipper.DebugHost.Models;

internal sealed record ProtoPayloadView(
    string Json,
    string Base64,
    int ByteCount);
