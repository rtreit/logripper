using Avalonia;

namespace QsoRipper.Gui.Utilities;

internal static class ResponsiveWindowLayout
{
    private const double SafeInsetDip = 16;

    public static WindowLayout ClampToWorkingArea(
        double preferredWidth,
        double preferredHeight,
        double preferredMinWidth,
        double preferredMinHeight,
        PixelRect workingArea,
        double scaling)
    {
        if (scaling <= 0)
        {
            scaling = 1;
        }

        var availableWidth = Math.Max(1, (workingArea.Width / scaling) - (SafeInsetDip * 2));
        var availableHeight = Math.Max(1, (workingArea.Height / scaling) - (SafeInsetDip * 2));

        var minWidth = Math.Min(preferredMinWidth, availableWidth);
        var minHeight = Math.Min(preferredMinHeight, availableHeight);
        var width = Math.Max(minWidth, Math.Min(preferredWidth, availableWidth));
        var height = Math.Max(minHeight, Math.Min(preferredHeight, availableHeight));

        var widthPixels = (int)Math.Round(width * scaling);
        var heightPixels = (int)Math.Round(height * scaling);
        var x = workingArea.X + Math.Max(0, (workingArea.Width - widthPixels) / 2);
        var y = workingArea.Y + Math.Max(0, (workingArea.Height - heightPixels) / 2);

        return new WindowLayout(width, height, minWidth, minHeight, new PixelPoint(x, y));
    }
}

internal readonly record struct WindowLayout(
    double Width,
    double Height,
    double MinWidth,
    double MinHeight,
    PixelPoint Position);
