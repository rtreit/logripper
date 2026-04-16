using QsoRipper.Domain;

namespace QsoRipper.Engine.RigControl.Tests;

public sealed class ModeMappingTests
{
    [Theory]
    [InlineData("USB", Mode.Ssb, "USB")]
    [InlineData("LSB", Mode.Ssb, "LSB")]
    public void SsbModes(string raw, Mode expectedMode, string expectedSubmode)
    {
        var result = ModeMapping.HamlibModeToProto(raw);
        Assert.Equal(expectedMode, result.Mode);
        Assert.Equal(expectedSubmode, result.Submode);
    }

    [Fact]
    public void CwModeHasNoSubmode()
    {
        var result = ModeMapping.HamlibModeToProto("CW");
        Assert.Equal(Mode.Cw, result.Mode);
        Assert.Null(result.Submode);
    }

    [Fact]
    public void CwrModeHasSubmode()
    {
        var result = ModeMapping.HamlibModeToProto("CWR");
        Assert.Equal(Mode.Cw, result.Mode);
        Assert.Equal("CWR", result.Submode);
    }

    [Fact]
    public void AmMode()
    {
        var result = ModeMapping.HamlibModeToProto("AM");
        Assert.Equal(Mode.Am, result.Mode);
        Assert.Null(result.Submode);
    }

    [Theory]
    [InlineData("FM")]
    [InlineData("WFM")]
    public void FmModes(string raw)
    {
        var result = ModeMapping.HamlibModeToProto(raw);
        Assert.Equal(Mode.Fm, result.Mode);
        Assert.Null(result.Submode);
    }

    [Fact]
    public void RttyMode()
    {
        var result = ModeMapping.HamlibModeToProto("RTTY");
        Assert.Equal(Mode.Rtty, result.Mode);
        Assert.Null(result.Submode);
    }

    [Fact]
    public void RttyrModeHasSubmode()
    {
        var result = ModeMapping.HamlibModeToProto("RTTYR");
        Assert.Equal(Mode.Rtty, result.Mode);
        Assert.Equal("RTTYR", result.Submode);
    }

    [Theory]
    [InlineData("PKTUSB", "PKTUSB")]
    [InlineData("PKTLSB", "PKTLSB")]
    [InlineData("PKTFM", "PKTFM")]
    public void PacketModes(string raw, string expectedSubmode)
    {
        var result = ModeMapping.HamlibModeToProto(raw);
        Assert.Equal(Mode.Pkt, result.Mode);
        Assert.Equal(expectedSubmode, result.Submode);
    }

    [Fact]
    public void Ft8Mode()
    {
        var result = ModeMapping.HamlibModeToProto("FT8");
        Assert.Equal(Mode.Ft8, result.Mode);
        Assert.Null(result.Submode);
    }

    [Theory]
    [InlineData("PSK")]
    [InlineData("PSK31")]
    public void PskModes(string raw)
    {
        var result = ModeMapping.HamlibModeToProto(raw);
        Assert.Equal(Mode.Psk, result.Mode);
        Assert.Null(result.Submode);
    }

    [Theory]
    [InlineData("usb", Mode.Ssb)]
    [InlineData("cw", Mode.Cw)]
    [InlineData("pktusb", Mode.Pkt)]
    [InlineData("Ft8", Mode.Ft8)]
    public void CaseInsensitive(string raw, Mode expectedMode)
    {
        var result = ModeMapping.HamlibModeToProto(raw);
        Assert.Equal(expectedMode, result.Mode);
    }

    [Theory]
    [InlineData("SOMETHING_NEW")]
    [InlineData("DIGITAL")]
    [InlineData("")]
    public void UnknownModeMapsToUnspecified(string raw)
    {
        var result = ModeMapping.HamlibModeToProto(raw);
        Assert.Equal(Mode.Unspecified, result.Mode);
        Assert.Null(result.Submode);
    }
}
