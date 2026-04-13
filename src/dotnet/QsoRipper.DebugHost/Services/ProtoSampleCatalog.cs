using Google.Protobuf;
using QsoRipper.DebugHost.Models;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Services;

internal sealed class ProtoSampleCatalog
{
    private readonly ProtoSampleDefinition[] _definitions = BuildDefinitions();

    public IReadOnlyList<ProtoSampleDefinition> GetDefinitions()
    {
        return _definitions;
    }

    private static ProtoSampleDefinition[] BuildDefinitions()
    {
        return GetGeneratedMessageTypes()
            .OrderBy(GetNamespaceSortKey)
            .ThenBy(static type => type.Name, StringComparer.Ordinal)
            .Select(static type => new ProtoSampleDefinition(
                type.FullName ?? type.Name,
                type.Name,
                type,
                type == typeof(QsoRecord)))
            .ToArray();
    }

    private static IEnumerable<Type> GetGeneratedMessageTypes()
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
                && typeof(IMessage).IsAssignableFrom(type));
    }

    private static int GetNamespaceSortKey(Type type)
    {
        return type.Namespace switch
        {
            "QsoRipper.Domain" => 0,
            "QsoRipper.Services" => 1,
            _ => 2
        };
    }
}
