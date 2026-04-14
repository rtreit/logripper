using Avalonia.Styling;
using QsoRipper.Gui.Inspection;

namespace QsoRipper.Gui.Tests;

public class UxInspectionOptionsTests
{
    [Fact]
    public void TryParseReturnsNullWhenInspectFlagIsMissing()
    {
        var success = UxInspectionOptions.TryParse(["--help"], out var options, out var error);

        Assert.True(success);
        Assert.Null(options);
        Assert.Null(error);
    }

    [Fact]
    public void TryParseReadsInspectArguments()
    {
        var success = UxInspectionOptions.TryParse(
            [
                "--inspect",
                "--inspect-fixture", ".\\scripts\\fixtures\\ux-main-window.fixture.json",
                "--inspect-theme", "light"
            ],
            out var options,
            out var error);

        Assert.True(success);
        Assert.NotNull(options);
        Assert.Null(error);
        Assert.EndsWith("scripts\\fixtures\\ux-main-window.fixture.json", options!.FixturePath, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(UxInspectionSurface.MainWindow, options.Surface);
        Assert.Equal(ThemeVariant.Light, options.ThemeVariant);
    }

    [Fact]
    public void TryParseRejectsUnknownTheme()
    {
        var success = UxInspectionOptions.TryParse(
            ["--inspect", "--inspect-theme", "blue"],
            out var options,
            out var error);

        Assert.False(success);
        Assert.Null(options);
        Assert.Equal("Unsupported inspect theme 'blue'. Use Default, Dark, or Light.", error);
    }

    [Fact]
    public void TryParseReadsInspectSurface()
    {
        var success = UxInspectionOptions.TryParse(
            ["--inspect", "--inspect-surface", "settings"],
            out var options,
            out var error);

        Assert.True(success);
        Assert.NotNull(options);
        Assert.Null(error);
        Assert.Equal(UxInspectionSurface.Settings, options!.Surface);
    }
}
