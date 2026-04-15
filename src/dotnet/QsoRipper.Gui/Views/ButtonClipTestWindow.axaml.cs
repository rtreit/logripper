using System;
using System.IO;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Media.Imaging;
using Avalonia.Threading;
using QsoRipper.Gui.Inspection;

namespace QsoRipper.Gui.Views;

internal sealed partial class ButtonClipTestWindow : Window
{
    public ButtonClipTestWindow()
    {
        InitializeComponent();
    }

    internal static Window CreateForCapture(
        IClassicDesktopStyleApplicationLifetime desktop,
        UxCaptureOptions options)
    {
        var window = new ButtonClipTestWindow();

        window.Opened += async (_, _) =>
        {
            try
            {
                await WaitForRenderAsync();
                await CaptureAndShutdownAsync(window, desktop, options);
            }
            catch (InvalidOperationException ex)
            {
                Console.Error.WriteLine($"Diagnostic capture failed: {ex.Message}");
                desktop.Shutdown(1);
            }
            catch (IOException ex)
            {
                Console.Error.WriteLine($"Diagnostic capture failed: {ex.Message}");
                desktop.Shutdown(1);
            }
        };

        return window;
    }

    private static async Task CaptureAndShutdownAsync(
        Window window,
        IClassicDesktopStyleApplicationLifetime desktop,
        UxCaptureOptions options)
    {
        var logicalSize = new Size(window.Bounds.Width, window.Bounds.Height);
        if (logicalSize.Width <= 0 || logicalSize.Height <= 0)
        {
            throw new InvalidOperationException("Diagnostic window has no rendered size.");
        }

        var pixelSize = new PixelSize(
            Math.Max(1, (int)Math.Ceiling(logicalSize.Width * window.RenderScaling)),
            Math.Max(1, (int)Math.Ceiling(logicalSize.Height * window.RenderScaling)));

        Directory.CreateDirectory(Path.GetDirectoryName(options.OutputPath)!);

        using var bitmap = new RenderTargetBitmap(
            pixelSize,
            new Vector(96 * window.RenderScaling, 96 * window.RenderScaling));
        bitmap.Render(window);
        bitmap.Save(options.OutputPath);

        Console.WriteLine($"Diagnostic captured: {options.OutputPath}");
        Console.WriteLine($"  Logical: {logicalSize.Width}x{logicalSize.Height}");
        Console.WriteLine($"  Pixel: {pixelSize.Width}x{pixelSize.Height}");
        Console.WriteLine($"  Scale: {window.RenderScaling}");

        desktop.Shutdown(0);
    }

    private static async Task WaitForRenderAsync()
    {
        await Dispatcher.UIThread.InvokeAsync(static () => { }, DispatcherPriority.Render);
        await Task.Delay(100);
        await Dispatcher.UIThread.InvokeAsync(static () => { }, DispatcherPriority.Background);
    }
}
