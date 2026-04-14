using System;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
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
            var endpoint = Environment.GetEnvironmentVariable("QSORIPPER_ENDPOINT")
                ?? "http://127.0.0.1:50051";

            var mainVm = new MainWindowViewModel(endpoint);

            desktop.MainWindow = new MainWindow
            {
                DataContext = mainVm
            };
        }

        base.OnFrameworkInitializationCompleted();
    }
}
