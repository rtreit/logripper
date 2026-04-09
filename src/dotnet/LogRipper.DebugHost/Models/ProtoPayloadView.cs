namespace LogRipper.DebugHost.Models;

public sealed record ProtoPayloadView(
    string Json,
    string Base64,
    int ByteCount);
