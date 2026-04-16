using System.Reflection;
using QsoRipper.Domain;

namespace QsoRipper.Engine.Lookup;

/// <summary>
/// Enriches <see cref="CallsignRecord"/> with DXCC entity data and zone information.
/// Matches the Rust engine's enrichment cascade:
/// 1. Zone from location (state/province subdivision)
/// 2. Zone from coordinates (lat/lon)
/// 3. Zone from grid-square fallback
/// 4. DXCC entity defaults (continent, country name, CQ zone, ITU zone)
/// </summary>
internal static class DxccEnrichment
{
    private static readonly Dictionary<uint, DxccEntityInfo> Entities = LoadEntities();

    /// <summary>
    /// Enrich missing CQ zone from location data (state/province or coordinates),
    /// then fill remaining gaps from the DXCC entity table.
    /// </summary>
    public static void Enrich(CallsignRecord record)
    {
        EnrichZonesFromLocation(record);
        EnrichFromDxccEntity(record);
    }

    /// <summary>
    /// Derive missing CQ zone from location data (state/province or coordinates).
    /// Matches Rust <c>enrich_zones_from_location</c>.
    /// </summary>
    internal static void EnrichZonesFromLocation(CallsignRecord record)
    {
        if (record.HasCqZone)
        {
            return;
        }

        // Step 1: Try state/subdivision mapping (most accurate for supported entities).
        var zone = CqZoneFromSubdivision(record.DxccEntityId, record.State);
        if (zone.HasValue)
        {
            record.CqZone = zone.Value;
            return;
        }

        // Step 2: Try coordinate-based derivation.
        double lat, lon;
        if (record.HasLatitude && record.HasLongitude)
        {
            lat = record.Latitude;
            lon = record.Longitude;
        }
        else
        {
            // Grid square → lat/lon fallback
            var coords = Maidenhead.GridToLatLon(record.HasGridSquare ? record.GridSquare : null);
            if (coords is null)
            {
                return;
            }

            (lat, lon) = coords.Value;
        }

        var coordZone = CqZoneFromCoordinates(record.DxccEntityId, lat, lon);
        if (coordZone.HasValue)
        {
            record.CqZone = coordZone.Value;
        }
    }

    /// <summary>
    /// Fill missing continent, CQ zone, ITU zone, and country name from the DXCC entity table.
    /// Matches Rust <c>enrich_callsign_record_from_dxcc</c>.
    /// </summary>
    internal static void EnrichFromDxccEntity(CallsignRecord record)
    {
        var dxcc = record.DxccEntityId;
        if (dxcc == 0)
        {
            return;
        }

        if (!Entities.TryGetValue(dxcc, out var entity))
        {
            return;
        }

        if (StringIsBlank(record.HasDxccContinent ? record.DxccContinent : null) && !string.IsNullOrEmpty(entity.Continent))
        {
            record.DxccContinent = entity.Continent;
        }

        if (!record.HasCqZone && entity.CqZone.HasValue)
        {
            record.CqZone = entity.CqZone.Value;
        }

        if (!record.HasItuZone && entity.ItuZone.HasValue)
        {
            record.ItuZone = entity.ItuZone.Value;
        }

        if (StringIsBlank(record.HasDxccCountryName ? record.DxccCountryName : null) && !string.IsNullOrEmpty(entity.Country))
        {
            record.DxccCountryName = entity.Country;
        }
    }

    /// <summary>
    /// Map a US state, Canadian province, or Australian state/territory to its CQ zone.
    /// </summary>
    internal static uint? CqZoneFromSubdivision(uint dxcc, string? state)
    {
        if (string.IsNullOrWhiteSpace(state))
        {
            return null;
        }

        var upper = state.Trim().ToUpperInvariant();

        return dxcc switch
        {
            // USA (lower 48 + DC) – DXCC 291
            291 => upper switch
            {
                "AZ" or "CA" or "ID" or "MT" or "NV" or "NM" or "OR" or "UT" or "WA" or "WY" => 3,
                "CO" or "AR" or "IA" or "KS" or "LA" or "MN" or "MO" or "MS" or "NE" or "ND" or "OK" or "SD" or "TX" or "WI" => 4,
                "AL" or "CT" or "DC" or "DE" or "FL" or "GA" or "IL" or "IN" or "KY" or "MA" or "MD" or "ME"
                    or "MI" or "NC" or "NH" or "NJ" or "NY" or "OH" or "PA" or "RI" or "SC" or "TN" or "VA" or "VT" or "WV" => 5,
                _ => null,
            },
            // Canada – DXCC 1
            1 => upper switch
            {
                "YT" or "NT" or "NU" => 1,
                "NL" => 2,
                "BC" => 3,
                "AB" or "SK" or "MB" or "ON" or "QC" or "NB" or "NS" or "PE" => 4,
                _ => null,
            },
            // Australia – DXCC 150
            150 => upper switch
            {
                "WA" => 29,
                "NT" or "SA" or "QLD" or "NSW" or "VIC" or "TAS" or "ACT" => 30,
                _ => null,
            },
            _ => null,
        };
    }

    /// <summary>
    /// Approximate CQ zone from latitude/longitude when state data is unavailable.
    /// </summary>
    internal static uint? CqZoneFromCoordinates(uint dxcc, double lat, double lon)
    {
        return dxcc switch
        {
            // USA (lower 48 + DC)
            291 => lon <= -105.0 ? 3u : lon <= -90.0 ? 4u : 5u,
            // Canada
            1 => (lat > 60.0 && lon < -110.0) ? 1u
               : (lat > 53.0 && lon > -66.0) ? 2u
               : lon < -110.0 ? 3u
               : 4u,
            // Australia
            150 => lon < 130.0 ? 29u : 30u,
            _ => null,
        };
    }

    private static bool StringIsBlank(string? value)
    {
        return string.IsNullOrWhiteSpace(value);
    }

    private static Dictionary<uint, DxccEntityInfo> LoadEntities()
    {
        var assembly = typeof(DxccEnrichment).Assembly;
        using var stream = assembly.GetManifestResourceStream("QsoRipper.Engine.Lookup.dxcc_entities.tsv")
            ?? throw new InvalidOperationException("Embedded dxcc_entities.tsv not found.");
        using var reader = new StreamReader(stream);

        var entities = new Dictionary<uint, DxccEntityInfo>();
        while (reader.ReadLine() is { } line)
        {
            if (string.IsNullOrWhiteSpace(line))
            {
                continue;
            }

            var parts = line.Split('\t');
            if (parts.Length < 3)
            {
                continue;
            }

            if (!uint.TryParse(parts[0], out var code))
            {
                continue;
            }

            var country = parts[1];
            var continent = parts[2];
            uint? cqZone = parts.Length > 3 && uint.TryParse(parts[3], out var cq) ? cq : null;
            uint? ituZone = parts.Length > 4 && uint.TryParse(parts[4], out var itu) ? itu : null;

            entities[code] = new DxccEntityInfo(country, continent, cqZone, ituZone);
        }

        return entities;
    }

    private sealed record DxccEntityInfo(string Country, string Continent, uint? CqZone, uint? ItuZone);
}
