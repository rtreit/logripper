using System;
using Avalonia;
using QsoRipper.Gui.Inspection;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui;

internal static class Program
{
    internal static UxCaptureOptions? CaptureOptions { get; private set; }

    internal static UxInspectionOptions? InspectionOptions { get; private set; }

    [STAThread]
    public static void Main(string[] args)
    {
        GuiPerformanceTrace.Write(nameof(Main) + ".start");
        if (!UxCaptureOptions.TryParse(args, out var options, out var error))
        {
            Console.Error.WriteLine(error);
            Environment.ExitCode = 1;
            return;
        }

        if (!UxInspectionOptions.TryParse(args, out var inspectionOptions, out error))
        {
            Console.Error.WriteLine(error);
            Environment.ExitCode = 1;
            return;
        }

        if (options is not null && inspectionOptions is not null)
        {
            Console.Error.WriteLine("Cannot combine --capture and --inspect.");
            Environment.ExitCode = 1;
            return;
        }

        CaptureOptions = options;
        InspectionOptions = inspectionOptions;

        GuiPerformanceTrace.Write(nameof(Main) + ".beforeStartWithClassicDesktopLifetime");
        BuildAvaloniaApp()
            .StartWithClassicDesktopLifetime(args);
    }

    public static AppBuilder BuildAvaloniaApp()
        => AppBuilder.Configure<App>()
            .UsePlatformDetect()
            .LogToTrace();
}
