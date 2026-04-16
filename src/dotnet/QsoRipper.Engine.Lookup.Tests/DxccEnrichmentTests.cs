using QsoRipper.Domain;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class DxccEnrichmentTests
{
    // ── US state mapping ────────────────────────────────────────────────

    [Theory]
    [InlineData("WA", 3u)]
    [InlineData("CA", 3u)]
    [InlineData("OR", 3u)]
    [InlineData("TX", 4u)]
    [InlineData("CO", 4u)]
    [InlineData("NY", 5u)]
    [InlineData("FL", 5u)]
    public void UsState_MapsToCorrectCqZone(string state, uint expectedZone)
    {
        var record = new CallsignRecord { DxccEntityId = 291 };
        record.State = state;

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(expectedZone, record.CqZone);
    }

    // ── Canadian province mapping ───────────────────────────────────────

    [Theory]
    [InlineData("YT", 1u)]
    [InlineData("NL", 2u)]
    [InlineData("BC", 3u)]
    [InlineData("ON", 4u)]
    public void CanadianProvince_MapsToCorrectCqZone(string province, uint expectedZone)
    {
        var record = new CallsignRecord { DxccEntityId = 1 };
        record.State = province;

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(expectedZone, record.CqZone);
    }

    // ── Australian state mapping ────────────────────────────────────────

    [Theory]
    [InlineData("WA", 29u)]
    [InlineData("NSW", 30u)]
    [InlineData("VIC", 30u)]
    public void AustralianState_MapsToCorrectCqZone(string state, uint expectedZone)
    {
        var record = new CallsignRecord { DxccEntityId = 150 };
        record.State = state;

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(expectedZone, record.CqZone);
    }

    // ── Coordinate fallback ─────────────────────────────────────────────

    [Fact]
    public void Us_WesternCoordinates_MapToZone3()
    {
        var record = new CallsignRecord { DxccEntityId = 291, Latitude = 47.5, Longitude = -122.5 };

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(3u, record.CqZone);
    }

    [Fact]
    public void Us_CentralCoordinates_MapToZone4()
    {
        var record = new CallsignRecord { DxccEntityId = 291, Latitude = 32.0, Longitude = -97.0 };

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(4u, record.CqZone);
    }

    [Fact]
    public void Us_EasternCoordinates_MapToZone5()
    {
        var record = new CallsignRecord { DxccEntityId = 291, Latitude = 40.7, Longitude = -74.0 };

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(5u, record.CqZone);
    }

    // ── Grid-square fallback ────────────────────────────────────────────

    [Fact]
    public void Us_GridCN87_MapsToZone3()
    {
        var record = new CallsignRecord { DxccEntityId = 291, GridSquare = "CN87" };

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(3u, record.CqZone);
    }

    // ── Existing zone preserved ─────────────────────────────────────────

    [Fact]
    public void ExistingZone_IsPreserved()
    {
        var record = new CallsignRecord { DxccEntityId = 291, CqZone = 99 };
        record.State = "WA";

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(99u, record.CqZone);
    }

    // ── State takes priority over coordinates ───────────────────────────

    [Fact]
    public void State_TakesPriorityOverCoordinates()
    {
        var record = new CallsignRecord
        {
            DxccEntityId = 291,
            Latitude = 40.0,
            Longitude = -74.0,
        };
        record.State = "WA"; // WA state → zone 3

        DxccEnrichment.EnrichZonesFromLocation(record);

        Assert.Equal(3u, record.CqZone);
    }

    // ── DXCC entity enrichment ──────────────────────────────────────────

    [Fact]
    public void EnrichFromDxcc_FillsContinent()
    {
        var record = new CallsignRecord { DxccEntityId = 291 };

        DxccEnrichment.EnrichFromDxccEntity(record);

        Assert.Equal("NA", record.DxccContinent);
        Assert.Equal("UNITED STATES OF AMERICA", record.DxccCountryName);
    }

    [Fact]
    public void EnrichFromDxcc_FillsDefaultCqZone()
    {
        var record = new CallsignRecord { DxccEntityId = 291 };

        DxccEnrichment.EnrichFromDxccEntity(record);

        Assert.True(record.HasCqZone);
        Assert.True(record.HasItuZone);
    }

    [Fact]
    public void EnrichFromDxcc_DoesNotOverwriteExistingContinent()
    {
        var record = new CallsignRecord { DxccEntityId = 291, DxccContinent = "XX" };

        DxccEnrichment.EnrichFromDxccEntity(record);

        Assert.Equal("XX", record.DxccContinent);
    }

    [Fact]
    public void EnrichFromDxcc_ZeroDxcc_NoOp()
    {
        var record = new CallsignRecord { DxccEntityId = 0 };

        DxccEnrichment.EnrichFromDxccEntity(record);

        Assert.False(record.HasDxccContinent);
    }

    [Fact]
    public void EnrichFromDxcc_UnknownDxcc_NoOp()
    {
        var record = new CallsignRecord { DxccEntityId = 99999 };

        DxccEnrichment.EnrichFromDxccEntity(record);

        Assert.False(record.HasDxccContinent);
    }

    // ── Full enrichment cascade ─────────────────────────────────────────

    [Fact]
    public void Enrich_FullCascade_ZoneFromState_ThenDxccDefaults()
    {
        var record = new CallsignRecord { DxccEntityId = 291 };
        record.State = "WA";

        DxccEnrichment.Enrich(record);

        Assert.Equal(3u, record.CqZone); // From WA state mapping, not DXCC default
        Assert.Equal("NA", record.DxccContinent); // From DXCC entity table
        Assert.Equal("UNITED STATES OF AMERICA", record.DxccCountryName);
    }
}
