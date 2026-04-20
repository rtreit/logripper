using System.Text.Json;
using QsoRipper.EngineSelection;

namespace QsoRipper.Cli.Tests;

public sealed class EngineRuntimeDiscoveryTests
{
    [Fact]
    public void DiscoverLocalEnginesReadsModernStateFiles()
    {
        var runtimeDirectory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        Directory.CreateDirectory(runtimeDirectory);
        try
        {
            var statePath = Path.Combine(runtimeDirectory, "qsoripper-local-dotnet.state.json");
            WriteState(statePath, new
            {
                displayName = "QsoRipper .NET Engine",
                engine = KnownEngineProfiles.LocalDotNet,
                engineId = "dotnet-aspnet",
                listenAddress = "127.0.0.1:50052",
                pid = Environment.ProcessId,
                startedAtUtc = DateTimeOffset.UtcNow.ToString("O"),
            });

            var entries = EngineRuntimeDiscovery.DiscoverLocalEngines(new EngineRuntimeDiscoveryOptions
            {
                RuntimeDirectory = runtimeDirectory,
                ValidateTcpReachability = false,
            });

            var entry = Assert.Single(entries);
            Assert.Equal(KnownEngineProfiles.LocalDotNet, entry.Profile.ProfileId);
            Assert.Equal("http://127.0.0.1:50052", entry.Endpoint);
            Assert.True(entry.IsProcessAlive);
            Assert.True(entry.IsRunning);
        }
        finally
        {
            Directory.Delete(runtimeDirectory, recursive: true);
        }
    }

    [Fact]
    public void DiscoverLocalEnginesMarksStaleProcessEntriesAsNotRunning()
    {
        var runtimeDirectory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        Directory.CreateDirectory(runtimeDirectory);
        try
        {
            var statePath = Path.Combine(runtimeDirectory, "qsoripper-local-rust.state.json");
            WriteState(statePath, new
            {
                displayName = "QsoRipper Rust Engine",
                engine = KnownEngineProfiles.LocalRust,
                engineId = "rust-tonic",
                listenAddress = "127.0.0.1:50051",
                pid = int.MaxValue,
                startedAtUtc = DateTimeOffset.UtcNow.ToString("O"),
            });

            var entries = EngineRuntimeDiscovery.DiscoverLocalEngines(new EngineRuntimeDiscoveryOptions
            {
                RuntimeDirectory = runtimeDirectory,
                ValidateTcpReachability = false,
            });

            var entry = Assert.Single(entries);
            Assert.False(entry.IsProcessAlive);
            Assert.False(entry.IsRunning);
        }
        finally
        {
            Directory.Delete(runtimeDirectory, recursive: true);
        }
    }

    [Fact]
    public void DiscoverLocalEnginesPrefersMostRecentStatePerProfile()
    {
        var runtimeDirectory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        Directory.CreateDirectory(runtimeDirectory);
        try
        {
            WriteState(Path.Combine(runtimeDirectory, "qsoripper-engine.json"), new
            {
                displayName = "QsoRipper Rust Engine",
                engine = KnownEngineProfiles.LocalRust,
                engineId = "rust-tonic",
                listenAddress = "127.0.0.1:50051",
                pid = Environment.ProcessId,
                startedAtUtc = DateTimeOffset.UtcNow.AddMinutes(-2).ToString("O"),
            });
            WriteState(Path.Combine(runtimeDirectory, "qsoripper-engine-local-rust.json"), new
            {
                displayName = "QsoRipper Rust Engine",
                engine = KnownEngineProfiles.LocalRust,
                engineId = "rust-tonic",
                listenAddress = "127.0.0.1:60051",
                pid = Environment.ProcessId,
                startedAtUtc = DateTimeOffset.UtcNow.ToString("O"),
            });

            var entries = EngineRuntimeDiscovery.DiscoverLocalEngines(new EngineRuntimeDiscoveryOptions
            {
                RuntimeDirectory = runtimeDirectory,
                ValidateTcpReachability = false,
            });

            var entry = Assert.Single(entries);
            Assert.Equal("http://127.0.0.1:60051", entry.Endpoint);
        }
        finally
        {
            Directory.Delete(runtimeDirectory, recursive: true);
        }
    }

    [Fact]
    public void DiscoverLocalEnginesTreatsConnectionRefusedAsNotRunning()
    {
        var runtimeDirectory = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        Directory.CreateDirectory(runtimeDirectory);
        try
        {
            WriteState(Path.Combine(runtimeDirectory, "qsoripper-engine-local-dotnet.json"), new
            {
                displayName = "QsoRipper .NET Engine",
                engine = KnownEngineProfiles.LocalDotNet,
                engineId = "dotnet-aspnet",
                listenAddress = "127.0.0.1:1",
                pid = Environment.ProcessId,
                startedAtUtc = DateTimeOffset.UtcNow.ToString("O"),
            });

            var entries = EngineRuntimeDiscovery.DiscoverLocalEngines(new EngineRuntimeDiscoveryOptions
            {
                RuntimeDirectory = runtimeDirectory,
                ValidateTcpReachability = true,
                TcpProbeTimeout = TimeSpan.FromMilliseconds(100),
            });

            var entry = Assert.Single(entries);
            Assert.True(entry.IsProcessAlive);
            Assert.False(entry.IsRunning);
        }
        finally
        {
            Directory.Delete(runtimeDirectory, recursive: true);
        }
    }

    private static void WriteState(string path, object payload)
    {
        File.WriteAllText(path, JsonSerializer.Serialize(payload));
    }
}
