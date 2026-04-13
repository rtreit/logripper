namespace QsoRipper.DebugHost.Models;

internal enum EngineProbeStage
{
    InvalidEndpoint,
    TcpUnreachable,
    GrpcUnavailable,
    MethodUnimplemented,
    MethodSucceeded,
}
