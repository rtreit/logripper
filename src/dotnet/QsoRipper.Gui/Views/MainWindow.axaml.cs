using Avalonia;
using Avalonia.Controls;
using Avalonia.Threading;
using QsoRipper.Gui.Utilities;

namespace QsoRipper.Gui.Views;

internal sealed partial class MainWindow : Window
{
    private readonly MenuItem? _fileMenuItem;
    private readonly TextBox? _recentQsoSearchBox;
    private readonly double _preferredWidth;
    private readonly double _preferredHeight;
    private readonly double _preferredMinWidth;
    private readonly double _preferredMinHeight;
    private ViewModels.MainWindowViewModel? _viewModel;
    private bool _menuAccessKeysPrimed;

    public MainWindow()
    {
        InitializeComponent();
        _preferredWidth = Width;
        _preferredHeight = Height;
        _preferredMinWidth = MinWidth;
        _preferredMinHeight = MinHeight;
        _fileMenuItem = this.FindControl<MenuItem>("FileMenuItem");
        _recentQsoSearchBox = this.FindControl<TextBox>("RecentQsoSearchBox");
        DataContextChanged += OnDataContextChanged;
    }

    protected override async void OnOpened(System.EventArgs e)
    {
        base.OnOpened(e);
        ClampToCurrentScreen();
        PrimeMenuAccessKeys();
        if (DataContext is ViewModels.MainWindowViewModel vm)
        {
            await vm.CheckFirstRunAsync();
        }
    }

    protected override void OnClosed(EventArgs e)
    {
        if (_viewModel is not null)
        {
            _viewModel.SearchFocusRequested -= OnSearchFocusRequested;
            _viewModel = null;
        }

        if (DataContext is IDisposable disposable)
        {
            disposable.Dispose();
        }

        base.OnClosed(e);
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

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (_viewModel is not null)
        {
            _viewModel.SearchFocusRequested -= OnSearchFocusRequested;
        }

        _viewModel = DataContext as ViewModels.MainWindowViewModel;
        if (_viewModel is not null)
        {
            _viewModel.SearchFocusRequested += OnSearchFocusRequested;
        }
    }

    private void OnSearchFocusRequested(object? sender, EventArgs e)
    {
        FocusRecentQsoSearchBox();
    }

    private void FocusRecentQsoSearchBox()
    {
        if (_recentQsoSearchBox is null)
        {
            return;
        }

        Dispatcher.UIThread.Post(
            () =>
            {
                _recentQsoSearchBox.Focus();
                _recentQsoSearchBox.SelectAll();
            },
            DispatcherPriority.Input);
    }

    private void ClampToCurrentScreen()
    {
        var screen = Screens.ScreenFromWindow(this) ?? Screens.Primary;
        if (screen is null)
        {
            return;
        }

        var layout = ResponsiveWindowLayout.ClampToWorkingArea(
            _preferredWidth,
            _preferredHeight,
            _preferredMinWidth,
            _preferredMinHeight,
            screen.WorkingArea,
            screen.Scaling);

        MinWidth = layout.MinWidth;
        MinHeight = layout.MinHeight;
        Width = layout.Width;
        Height = layout.Height;
        Position = layout.Position;
    }
}
