using Google.Protobuf;

namespace QsoRipper.DebugHost.Models;

internal sealed record RigControlInvocationResult(
    string InvocationMode,
    bool Succeeded,
    string? ErrorMessage,
    IMessage? Response,
    DateTimeOffset CompletedAtUtc);
