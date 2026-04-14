using Avalonia;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.Tests;

public class ResponsiveWindowLayoutTests
{
    [Fact]
    public void ClampToWorkingAreaKeepsPreferredSizeWhenWorkingAreaIsLargeEnough()
    {
        var layout = ResponsiveWindowLayout.ClampToWorkingArea(
            preferredWidth: 1280,
            preferredHeight: 760,
            preferredMinWidth: 1024,
            preferredMinHeight: 640,
            workingArea: new PixelRect(0, 0, 1920, 1080),
            scaling: 1);

        Assert.Equal(1280, layout.Width);
        Assert.Equal(760, layout.Height);
        Assert.Equal(1024, layout.MinWidth);
        Assert.Equal(640, layout.MinHeight);
        Assert.Equal(new PixelPoint(320, 160), layout.Position);
    }

    [Fact]
    public void ClampToWorkingAreaShrinksWindowAndMinimumsOnSmallDisplay()
    {
        var layout = ResponsiveWindowLayout.ClampToWorkingArea(
            preferredWidth: 1280,
            preferredHeight: 760,
            preferredMinWidth: 1024,
            preferredMinHeight: 640,
            workingArea: new PixelRect(0, 0, 1366, 768),
            scaling: 1);

        Assert.Equal(1024, layout.MinWidth);
        Assert.Equal(640, layout.MinHeight);
        Assert.Equal(1280, layout.Width);
        Assert.Equal(736, layout.Height);
        Assert.Equal(new PixelPoint(43, 16), layout.Position);
    }

    [Fact]
    public void ClampToWorkingAreaAccountsForMonitorScaling()
    {
        var layout = ResponsiveWindowLayout.ClampToWorkingArea(
            preferredWidth: 1280,
            preferredHeight: 760,
            preferredMinWidth: 1024,
            preferredMinHeight: 640,
            workingArea: new PixelRect(0, 0, 1920, 1080),
            scaling: 1.5);

        Assert.Equal(1248, layout.Width);
        Assert.Equal(688, layout.Height);
        Assert.Equal(1024, layout.MinWidth);
        Assert.Equal(640, layout.MinHeight);
        Assert.Equal(new PixelPoint(24, 24), layout.Position);
    }
}
