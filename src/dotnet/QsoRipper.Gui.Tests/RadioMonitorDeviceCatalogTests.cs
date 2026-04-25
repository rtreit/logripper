using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.Tests;

public sealed class RadioMonitorDeviceCatalogTests
{
    [Fact]
    public void ParseWithValidJsonReturnsSystemDefaultPlusInputsAndLoopback()
    {
        const string json = """
        {
          "inputs": ["Microphone (USB Audio CODEC)", "Realtek HD Audio"],
          "loopback": ["Speakers (Realtek)", "Headphones (USB)"]
        }
        """;

        var devices = RadioMonitorDeviceCatalog.Parse(json);

        Assert.Equal(5, devices.Count);
        Assert.Same(RadioMonitorDeviceCatalog.SystemDefault, devices[0]);
        Assert.Equal("Microphone (USB Audio CODEC)", devices[1].Name);
        Assert.False(devices[1].IsLoopback);
        Assert.Equal("Realtek HD Audio", devices[2].Name);
        Assert.False(devices[2].IsLoopback);
        Assert.Equal("Speakers (Realtek)", devices[3].Name);
        Assert.True(devices[3].IsLoopback);
        Assert.Equal("Headphones (USB)", devices[4].Name);
        Assert.True(devices[4].IsLoopback);
    }

    [Fact]
    public void ParseWithEmptyArraysReturnsOnlySystemDefault()
    {
        const string json = """{"inputs":[],"loopback":[]}""";

        var devices = RadioMonitorDeviceCatalog.Parse(json);

        Assert.Single(devices);
        Assert.Same(RadioMonitorDeviceCatalog.SystemDefault, devices[0]);
    }

    [Fact]
    public void ParseWithMalformedJsonFallsBackToSystemDefaultOnly()
    {
        var devices = RadioMonitorDeviceCatalog.Parse("Available input devices:\n  - Mic 1");

        Assert.Single(devices);
        Assert.Same(RadioMonitorDeviceCatalog.SystemDefault, devices[0]);
    }

    [Fact]
    public void ParseWithEmptyStringReturnsSystemDefaultOnly()
    {
        var devices = RadioMonitorDeviceCatalog.Parse(string.Empty);

        Assert.Single(devices);
        Assert.Same(RadioMonitorDeviceCatalog.SystemDefault, devices[0]);
    }

    [Fact]
    public void DisplayNameLoopbackDeviceAppendsLoopbackHint()
    {
        var device = new RadioMonitorDevice("Speakers", IsLoopback: true);
        Assert.Contains("loopback", device.DisplayName, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void DisplayNameNormalDeviceReturnsName()
    {
        var device = new RadioMonitorDevice("USB Mic", IsLoopback: false);
        Assert.Equal("USB Mic", device.DisplayName);
    }
}
