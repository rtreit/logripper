using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.EngineSelection;

namespace QsoRipper.Cli.Tests;

public sealed class EngineReachabilityTests
{
    [Fact]
    public void SuggestedCommandReturnsCargoForRustProfile()
    {
        var command = EngineReachability.SuggestedCommand(EngineCatalog.RustProfile);

        Assert.Equal("cargo run --manifest-path src/rust/Cargo.toml -p qsoripper-server", command);
    }

    [Fact]
    public void SuggestedCommandReturnsDotnetForDotNetProfile()
    {
        var command = EngineReachability.SuggestedCommand(EngineCatalog.DotNetProfile);

        Assert.Equal("dotnet run --project src/dotnet/QsoRipper.Engine.DotNet", command);
    }

    [Fact]
    public void SuggestedCommandReturnsGenericForUnknownProfile()
    {
        var profile = new EngineTargetProfile(
            "remote-cluster",
            "remote",
            "Remote Cluster Engine",
            "http://example.test:50051",
            ["remote-cluster"],
            LocalLaunchRecipe: null);

        var command = EngineReachability.SuggestedCommand(profile);

        Assert.Equal("the engine binary for this profile", command);
    }

    [Fact]
    public void FormatUnreachableMessageIncludesEndpointProfileNameAndSuggestedCommand()
    {
        var profile = EngineCatalog.RustProfile;
        var endpoint = "http://127.0.0.1:50051";

        var message = EngineReachability.FormatUnreachableMessage(profile, endpoint);

        Assert.Contains(profile.DisplayName, message, StringComparison.Ordinal);
        Assert.Contains(endpoint, message, StringComparison.Ordinal);
        Assert.Contains(EngineReachability.SuggestedCommand(profile), message, StringComparison.Ordinal);
        Assert.Contains("Make sure the engine is running", message, StringComparison.Ordinal);
    }

    [Fact]
    public async Task ProbeAsyncReportsUnreachableForClosedPort()
    {
        using var channel = GrpcChannel.ForAddress("http://127.0.0.1:1");
        var profile = EngineCatalog.RustProfile;
        var endpoint = "http://127.0.0.1:1";

        var result = await EngineReachability.ProbeAsync(
            channel,
            profile,
            endpoint,
            deadline: TimeSpan.FromMilliseconds(500));

        Assert.False(result.IsReachable);
        Assert.NotNull(result.ErrorMessage);
        Assert.Contains(endpoint, result.ErrorMessage, StringComparison.Ordinal);
        Assert.Contains(profile.DisplayName, result.ErrorMessage, StringComparison.Ordinal);
    }
}
