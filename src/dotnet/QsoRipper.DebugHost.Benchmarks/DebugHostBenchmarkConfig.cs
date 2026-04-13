using System.Diagnostics.CodeAnalysis;
using BenchmarkDotNet.Configs;
using BenchmarkDotNet.Exporters;
using BenchmarkDotNet.Exporters.Csv;
using BenchmarkDotNet.Exporters.Json;
using BenchmarkDotNet.Jobs;

namespace QsoRipper.DebugHost.Benchmarks;

[SuppressMessage(
    "Performance",
    "CA1812:Avoid uninstantiated internal classes",
    Justification = "BenchmarkDotNet activates config types via reflection from the [Config] attribute.")]
internal sealed class DebugHostBenchmarkConfig : ManualConfig
{
    public DebugHostBenchmarkConfig()
    {
        AddJob(
            Job.Default
                .WithLaunchCount(1)
                .WithWarmupCount(2)
                .WithIterationCount(4)
                .WithId("Comparative"));

        AddExporter(MarkdownExporter.GitHub);
        AddExporter(CsvExporter.Default);
        AddExporter(CsvMeasurementsExporter.Default);
        AddExporter(JsonExporter.Full);
    }
}
