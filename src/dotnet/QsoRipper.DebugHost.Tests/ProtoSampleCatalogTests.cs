using Google.Protobuf;
using QsoRipper.DebugHost.Services;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class ProtoSampleCatalogTests
{
    private readonly ProtoSampleCatalog _catalog = new();

    [Fact]
    public void Catalog_includes_all_generated_shared_proto_messages()
    {
        var actual = _catalog.GetDefinitions()
            .Select(static definition => definition.MessageType)
            .ToHashSet();
        var expected = GetGeneratedMessageTypes();

        Assert.Equal(expected.Count, actual.Count);
        Assert.Subset(actual, expected);
        Assert.Subset(expected, actual);
    }

    private static HashSet<Type> GetGeneratedMessageTypes()
    {
        var assemblies = new[]
        {
            typeof(QsoRecord).Assembly,
            typeof(RuntimeConfigSnapshot).Assembly
        }.Distinct();

        return assemblies
            .SelectMany(static assembly => assembly.GetTypes())
            .Where(static type =>
                type is { IsClass: true, IsAbstract: false, IsPublic: true }
                && type.Namespace is "QsoRipper.Domain" or "QsoRipper.Services"
                && typeof(IMessage).IsAssignableFrom(type))
            .ToHashSet();
    }
}
#pragma warning restore CA1707
