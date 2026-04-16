using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl.Tests;

public sealed class BandMappingTests
{
    [Theory]
    [InlineData(1_840_000UL, Band._160M)]
    [InlineData(3_573_000UL, Band._80M)]
    [InlineData(5_357_000UL, Band._60M)]
    [InlineData(7_074_000UL, Band._40M)]
    [InlineData(10_136_000UL, Band._30M)]
    [InlineData(14_074_000UL, Band._20M)]
    [InlineData(18_100_000UL, Band._17M)]
    [InlineData(21_074_000UL, Band._15M)]
    [InlineData(24_915_000UL, Band._12M)]
    [InlineData(28_074_000UL, Band._10M)]
    public void CommonHfFrequencies(ulong hz, Band expected)
    {
        Assert.Equal(expected, BandMapping.FrequencyHzToBand(hz));
    }

    [Theory]
    [InlineData(50_313_000UL, Band._6M)]
    [InlineData(144_174_000UL, Band._2M)]
    [InlineData(432_065_000UL, Band._70Cm)]
    public void VhfUhfFrequencies(ulong hz, Band expected)
    {
        Assert.Equal(expected, BandMapping.FrequencyHzToBand(hz));
    }

    [Theory]
    [InlineData(14_000_000UL, Band._20M)]
    [InlineData(7_000_000UL, Band._40M)]
    [InlineData(3_500_000UL, Band._80M)]
    [InlineData(1_800_000UL, Band._160M)]
    [InlineData(50_000_000UL, Band._6M)]
    public void LowerBoundaryIsInclusive(ulong hz, Band expected)
    {
        Assert.Equal(expected, BandMapping.FrequencyHzToBand(hz));
    }

    [Theory]
    [InlineData(14_350_000UL, Band._20M)]
    [InlineData(7_300_000UL, Band._40M)]
    [InlineData(29_700_000UL, Band._10M)]
    [InlineData(2_000_000UL, Band._160M)]
    [InlineData(54_000_000UL, Band._6M)]
    public void UpperBoundaryIsInclusive(ulong hz, Band expected)
    {
        Assert.Equal(expected, BandMapping.FrequencyHzToBand(hz));
    }

    [Theory]
    [InlineData(0UL)]
    [InlineData(100_000UL)]
    [InlineData(13_999_999UL)]
    [InlineData(14_350_001UL)]
    [InlineData(5_000_000_000UL)]
    public void OutOfRangeReturnsUnspecified(ulong hz)
    {
        Assert.Equal(Band.Unspecified, BandMapping.FrequencyHzToBand(hz));
    }

    [Theory]
    [InlineData(136_000UL, Band._2190M)]
    [InlineData(475_000UL, Band._630M)]
    public void LowFrequencyBands(ulong hz, Band expected)
    {
        Assert.Equal(expected, BandMapping.FrequencyHzToBand(hz));
    }

    [Theory]
    [InlineData(903_000_000UL, Band._33Cm)]
    [InlineData(1_296_000_000UL, Band._23Cm)]
    public void MicrowaveBands(ulong hz, Band expected)
    {
        Assert.Equal(expected, BandMapping.FrequencyHzToBand(hz));
    }

    [Fact]
    public void Band125mRange()
    {
        Assert.Equal(Band._125M, BandMapping.FrequencyHzToBand(222_000_000));
    }
}
