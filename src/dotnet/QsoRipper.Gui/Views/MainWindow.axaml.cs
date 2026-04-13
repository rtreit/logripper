using Avalonia.Controls;

namespace QsoRipper.Gui.Views;

internal sealed partial class MainWindow : Window
{
    public MainWindow()
    {
        InitializeComponent();
    }

    protected override async void OnOpened(System.EventArgs e)
    {
        base.OnOpened(e);
        if (DataContext is ViewModels.MainWindowViewModel vm)
        {
            await vm.CheckFirstRunAsync();
        }
    }
}
