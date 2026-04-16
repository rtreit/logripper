using QsoRipper.Engine.Lookup;

namespace QsoRipper.Engine.Lookup.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class DxccEntityLookupTests
{
    // ── Known entity by code ────────────────────────────────────────────

    [Theory]
    [InlineData(291u, "UNITED STATES OF AMERICA", "NA")]
    [InlineData(1u, "CANADA", "NA")]
    [InlineData(150u, "AUSTRALIA", "OC")]
    [InlineData(223u, "ENGLAND", "EU")]
    [InlineData(339u, "JAPAN", "AS")]
    public void TryGetByCode_KnownEntities_ReturnExpectedData(uint code, string expectedCountry, string expectedContinent)
    {
        Assert.True(DxccEntityTable.TryGetByCode(code, out var entity));
        Assert.NotNull(entity);
        Assert.Equal(code, entity.DxccCode);
        Assert.Equal(expectedCountry, entity.CountryName);
        Assert.Equal(expectedContinent, entity.Continent);
    }

    // ── Zone data populated ─────────────────────────────────────────────

    [Fact]
    public void TryGetByCode_USA_HasZoneData()
    {
        Assert.True(DxccEntityTable.TryGetByCode(291, out var entity));
        Assert.True(entity!.HasCqZone);
        Assert.True(entity.HasItuZone);
    }

    // ── Unknown / zero codes ────────────────────────────────────────────

    [Fact]
    public void TryGetByCode_UnknownCode_ReturnsFalse()
    {
        Assert.False(DxccEntityTable.TryGetByCode(99999, out var entity));
        Assert.Null(entity);
    }

    [Fact]
    public void TryGetByCode_ZeroCode_ReturnsFalse()
    {
        Assert.False(DxccEntityTable.TryGetByCode(0, out _));
    }

    // ── Clone safety ────────────────────────────────────────────────────

    [Fact]
    public void TryGetByCode_ReturnsClone_MutationDoesNotAffectTable()
    {
        Assert.True(DxccEntityTable.TryGetByCode(291, out var first));
        first!.CountryName = "MODIFIED";

        Assert.True(DxccEntityTable.TryGetByCode(291, out var second));
        Assert.Equal("UNITED STATES OF AMERICA", second!.CountryName);
    }

    [Fact]
    public void TryGetByCode_TwoCallsReturnDistinctInstances()
    {
        Assert.True(DxccEntityTable.TryGetByCode(291, out var a));
        Assert.True(DxccEntityTable.TryGetByCode(291, out var b));
        Assert.NotSame(a, b);
    }
}
