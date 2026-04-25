using Grpc.Core;
using Grpc.Net.Client;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Cli;

internal readonly record struct EngineReachabilityResult(
    bool IsReachable,
    string? ErrorMessage,
    StatusCode? StatusCode);

internal static class EngineReachability
{
    private static readonly TimeSpan DefaultDeadline = TimeSpan.FromMilliseconds(1500);

    public static async Task<EngineReachabilityResult> ProbeAsync(
        GrpcChannel channel,
        EngineTargetProfile profile,
        string endpoint,
        TimeSpan? deadline = null,
        CancellationToken ct = default)
    {
        ArgumentNullException.ThrowIfNull(channel);
        ArgumentNullException.ThrowIfNull(profile);
        ArgumentNullException.ThrowIfNull(endpoint);

        var effectiveDeadline = deadline ?? DefaultDeadline;
        var deadlineUtc = DateTime.UtcNow.Add(effectiveDeadline);

        try
        {
            var engineClient = new EngineService.EngineServiceClient(channel);
            await engineClient
                .GetEngineInfoAsync(new GetEngineInfoRequest(), deadline: deadlineUtc, cancellationToken: ct)
                .ResponseAsync
                .ConfigureAwait(false);
            return new EngineReachabilityResult(true, null, null);
        }
        catch (RpcException ex) when (ex.StatusCode == Grpc.Core.StatusCode.Unimplemented)
        {
            return await ProbeWithLogbookAsync(channel, profile, endpoint, deadlineUtc, ct).ConfigureAwait(false);
        }
        catch (RpcException ex) when (IsUnreachable(ex.StatusCode))
        {
            return new EngineReachabilityResult(
                false,
                FormatUnreachableMessage(profile, endpoint),
                ex.StatusCode);
        }
        catch (RpcException ex)
        {
            return new EngineReachabilityResult(false, ex.Status.Detail, ex.StatusCode);
        }
    }

    public static string FormatUnreachableMessage(EngineTargetProfile profile, string endpoint)
    {
        ArgumentNullException.ThrowIfNull(profile);
        ArgumentNullException.ThrowIfNull(endpoint);

        return $"Could not connect to {profile.DisplayName} at {endpoint}.\nMake sure the engine is running. Suggested start command:\n  {SuggestedCommand(profile)}";
    }

    public static string SuggestedCommand(EngineTargetProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        if (string.Equals(profile.ProfileId, KnownEngineProfiles.LocalRust, StringComparison.OrdinalIgnoreCase)
            || profile.Matches("rust"))
        {
            return "cargo run --manifest-path src/rust/Cargo.toml -p qsoripper-server";
        }

        if (string.Equals(profile.ProfileId, KnownEngineProfiles.LocalDotNet, StringComparison.OrdinalIgnoreCase)
            || profile.Matches("dotnet"))
        {
            return "dotnet run --project src/dotnet/QsoRipper.Engine.DotNet";
        }

        return "the engine binary for this profile";
    }

    private static async Task<EngineReachabilityResult> ProbeWithLogbookAsync(
        GrpcChannel channel,
        EngineTargetProfile profile,
        string endpoint,
        DateTime deadlineUtc,
        CancellationToken ct)
    {
        try
        {
            var logbookClient = new LogbookService.LogbookServiceClient(channel);
            await logbookClient
                .GetSyncStatusAsync(new GetSyncStatusRequest(), deadline: deadlineUtc, cancellationToken: ct)
                .ResponseAsync
                .ConfigureAwait(false);
            return new EngineReachabilityResult(true, null, null);
        }
        catch (RpcException ex) when (IsUnreachable(ex.StatusCode))
        {
            return new EngineReachabilityResult(
                false,
                FormatUnreachableMessage(profile, endpoint),
                ex.StatusCode);
        }
        catch (RpcException ex)
        {
            return new EngineReachabilityResult(false, ex.Status.Detail, ex.StatusCode);
        }
    }

    private static bool IsUnreachable(StatusCode statusCode)
    {
        return statusCode is Grpc.Core.StatusCode.Unavailable
            or Grpc.Core.StatusCode.DeadlineExceeded
            or Grpc.Core.StatusCode.Internal;
    }
}
