using Avalonia.Styling;
using QsoRipper.Gui.Inspection;

namespace QsoRipper.Gui.Tests;

public class UxCaptureOptionsTests
{
    [Fact]
    public void TryParseReturnsNullWhenCaptureFlagIsMissing()
    {
        var success = UxCaptureOptions.TryParse(["--help"], out var options, out var error);

        Assert.True(success);
        Assert.Null(options);
        Assert.Null(error);
    }

    [Fact]
    public void TryParseReadsCaptureArguments()
    {
        var success = UxCaptureOptions.TryParse(
            [
                "--capture",
                "--capture-scenario", "recent-qso-grid",
                "--capture-target", "RecentQsoGrid",
                "--capture-output", ".\\artifacts\\ux\\current\\recent-qso-grid.png",
                "--capture-fixture", ".\\scripts\\fixtures\\ux-main-window.fixture.json",
                "--capture-theme", "light"
            ],
            out var options,
            out var error);

        Assert.True(success);
        Assert.NotNull(options);
        Assert.Null(error);
        Assert.Equal("recent-qso-grid", options!.Scenario);
        Assert.Equal("RecentQsoGrid", options.TargetName);
        Assert.EndsWith("artifacts\\ux\\current\\recent-qso-grid.png", options.OutputPath, StringComparison.OrdinalIgnoreCase);
        Assert.EndsWith("scripts\\fixtures\\ux-main-window.fixture.json", options.FixturePath, StringComparison.OrdinalIgnoreCase);
        Assert.Equal(ThemeVariant.Light, options.ThemeVariant);
    }

    [Fact]
    public void TryParseRejectsMissingOutput()
    {
        var success = UxCaptureOptions.TryParse(["--capture"], out var options, out var error);

        Assert.False(success);
        Assert.Null(options);
        Assert.Equal("Capture mode requires --capture-output <path>.", error);
    }
}
