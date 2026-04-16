using System.Diagnostics.CodeAnalysis;
using QsoRipper.Domain;

namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Read-only, thread-safe DXCC entity table backed by the embedded <c>dxcc_entities.tsv</c>.
/// Provides lookup by numeric DXCC code, returning a proto <see cref="DxccEntity"/> message.
/// </summary>
public static class DxccEntityTable
{
    private static readonly Dictionary<uint, DxccEntity> Entities = LoadEntities();

    /// <summary>Look up a DXCC entity by its numeric code.</summary>
    /// <returns><see langword="true"/> if found; the returned entity is a fresh clone safe for mutation.</returns>
    public static bool TryGetByCode(uint code, [NotNullWhen(true)] out DxccEntity? entity)
    {
        if (Entities.TryGetValue(code, out var stored))
        {
            // Clone so callers cannot mutate the shared table.
            entity = stored.Clone();
            return true;
        }

        entity = null;
        return false;
    }

    private static Dictionary<uint, DxccEntity> LoadEntities()
    {
        var assembly = typeof(DxccEntityTable).Assembly;
        using var stream = assembly.GetManifestResourceStream("QsoRipper.Engine.Lookup.dxcc_entities.tsv")
            ?? throw new InvalidOperationException("Embedded dxcc_entities.tsv not found.");
        using var reader = new StreamReader(stream);

        var entities = new Dictionary<uint, DxccEntity>();
        while (reader.ReadLine() is { } line)
        {
            if (string.IsNullOrWhiteSpace(line))
            {
                continue;
            }

            var parts = line.Split('\t');
            if (parts.Length < 3 || !uint.TryParse(parts[0], out var code))
            {
                continue;
            }

            var entry = new DxccEntity
            {
                DxccCode = code,
                CountryName = parts[1],
                Continent = parts[2],
            };

            if (parts.Length > 3 && uint.TryParse(parts[3], out var cq))
            {
                entry.CqZone = cq;
            }

            if (parts.Length > 4 && uint.TryParse(parts[4], out var itu))
            {
                entry.ItuZone = itu;
            }

            entities[code] = entry;
        }

        return entities;
    }
}
