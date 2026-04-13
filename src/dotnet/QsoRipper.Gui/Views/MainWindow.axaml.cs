using Avalonia.Controls;
using Avalonia.Threading;

namespace QsoRipper.Gui.Views;

internal sealed partial class MainWindow : Window
{
    private readonly MenuItem? _fileMenuItem;
    private bool _menuAccessKeysPrimed;

    public MainWindow()
    {
        InitializeComponent();
        _fileMenuItem = this.FindControl<MenuItem>("FileMenuItem");
    }

    protected override async void OnOpened(System.EventArgs e)
    {
        base.OnOpened(e);
        PrimeMenuAccessKeys();
        if (DataContext is ViewModels.MainWindowViewModel vm)
        {
            await vm.CheckFirstRunAsync();
        }
    }

    private void PrimeMenuAccessKeys()
    {
        if (_menuAccessKeysPrimed || _fileMenuItem is null)
        {
            return;
        }

        _menuAccessKeysPrimed = true;

        // Avalonia access-key mode does not fully initialize until a menu has been shown once.
        Dispatcher.UIThread.Post(
            () =>
            {
                _fileMenuItem.IsSubMenuOpen = true;
                Dispatcher.UIThread.Post(
                    () => _fileMenuItem.IsSubMenuOpen = false,
                    DispatcherPriority.Background);
            },
            DispatcherPriority.Background);
    }
}
