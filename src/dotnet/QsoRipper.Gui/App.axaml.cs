using System;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using QsoRipper.EngineSelection;
using QsoRipper.Gui.Inspection;
using QsoRipper.Gui.ViewModels;
using QsoRipper.Gui.Views;

namespace QsoRipper.Gui;

internal sealed partial class App : Application
{
    public override void Initialize()
    {
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            if (Program.CaptureOptions is { } captureOptions)
            {
                RequestedThemeVariant = captureOptions.ThemeVariant;
                desktop.MainWindow = captureOptions.Scenario.StartsWith("diag-", StringComparison.Ordinal)
                    ? Views.ButtonClipTestWindow.CreateForCapture(desktop, captureOptions)
                    : UxCaptureRunner.CreateWindow(desktop, captureOptions);
            }
            else if (Program.InspectionOptions is { } inspectionOptions)
            {
                RequestedThemeVariant = inspectionOptions.ThemeVariant;
                desktop.MainWindow = UxInspectionRunner.CreateWindow(desktop, inspectionOptions);
            }
            else
            {
                var engineProfile = EngineCatalog.ResolveProfile();
                var endpoint = EngineCatalog.ResolveEndpoint(engineProfile);

                var mainVm = new MainWindowViewModel(engineProfile, endpoint);

                desktop.MainWindow = new MainWindow
                {
                    DataContext = mainVm
                };
            }
        }

        base.OnFrameworkInitializationCompleted();
    }
}
