using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using CwDecoderGui.ViewModels;
using CwDecoderGui.Views;

namespace CwDecoderGui;

public partial class App : Application
{
    public override void Initialize() => AvaloniaXamlLoader.Load(this);

    public override void OnFrameworkInitializationCompleted()
    {
        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var vm = new MainWindowViewModel();
            desktop.MainWindow = new MainWindow { DataContext = vm };
            desktop.ShutdownRequested += (_, _) => vm.Dispose();

            // --file <path> (auto-open a file on startup, useful for screenshots)
            var args = desktop.Args ?? System.Array.Empty<string>();
            for (int i = 0; i < args.Length - 1; i++)
            {
                if (args[i] == "--file" && System.IO.File.Exists(args[i + 1]))
                {
                    var path = args[i + 1];
                    desktop.MainWindow.Opened += (_, _) =>
                        Avalonia.Threading.Dispatcher.UIThread.Post(async () => await vm.OpenFileAsync(path),
                            Avalonia.Threading.DispatcherPriority.Background);
                    break;
                }
            }
        }
        base.OnFrameworkInitializationCompleted();
    }
}
