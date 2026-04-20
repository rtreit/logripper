using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;

namespace QsoRipper.Gui.Views;

internal sealed partial class FullQsoCardView : UserControl
{
    private const int TabCount = 6;

    public FullQsoCardView()
    {
        InitializeComponent();
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        Dispatcher.UIThread.Post(FocusInitialField, DispatcherPriority.Background);
    }

    internal void FocusInitialField() => FocusCurrentTab();

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if ((e.KeyModifiers & KeyModifiers.Control) == KeyModifiers.Control && e.Key == Key.Tab)
        {
            ChangeTab((e.KeyModifiers & KeyModifiers.Shift) == KeyModifiers.Shift ? -1 : 1);
            e.Handled = true;
            return;
        }

        if ((e.KeyModifiers & KeyModifiers.Alt) == KeyModifiers.Alt)
        {
            switch (e.Key)
            {
                case Key.D1:
                case Key.NumPad1:
                    FocusTab(0, "WorkedCallsignBox");
                    e.Handled = true;
                    return;
                case Key.D2:
                case Key.NumPad2:
                    FocusTab(1, "LookupOperatorCallsignBox");
                    e.Handled = true;
                    return;
                case Key.D3:
                case Key.NumPad3:
                    FocusTab(2, "QslSentStatusBox");
                    e.Handled = true;
                    return;
                case Key.D4:
                case Key.NumPad4:
                    FocusTab(3, "ContestIdBox");
                    e.Handled = true;
                    return;
                case Key.D5:
                case Key.NumPad5:
                    FocusTab(4, "StationCallsignSnapshotBox");
                    e.Handled = true;
                    return;
                case Key.D6:
                case Key.NumPad6:
                    FocusTab(5, "ExtraFieldsBox");
                    e.Handled = true;
                    return;
            }
        }

        base.OnKeyDown(e);
    }

    private void OnCardTabsSelectionChanged(object? sender, SelectionChangedEventArgs e)
    {
        if (DataContext is not ViewModels.FullQsoCardViewModel viewModel)
        {
            return;
        }

        Dispatcher.UIThread.Post(
            () => FocusTab(viewModel.SelectedTabIndex, GetFocusTargetName(viewModel.SelectedTabIndex)),
            DispatcherPriority.Background);
    }

    private void FocusCurrentTab()
    {
        if (DataContext is not ViewModels.FullQsoCardViewModel viewModel)
        {
            FocusTab(0, "WorkedCallsignBox");
            return;
        }

        FocusTab(viewModel.SelectedTabIndex, viewModel.SelectedTabIndex switch
        {
            _ => GetFocusTargetName(viewModel.SelectedTabIndex)
        });
    }

    private void ChangeTab(int delta)
    {
        if (DataContext is not ViewModels.FullQsoCardViewModel viewModel)
        {
            return;
        }

        var nextIndex = (viewModel.SelectedTabIndex + delta + TabCount) % TabCount;
        FocusTab(nextIndex, GetFocusTargetName(nextIndex));
    }

    private void FocusTab(int index, string targetName)
    {
        if (this.FindControl<TabControl>("CardTabs") is { } tabs)
        {
            tabs.SelectedIndex = index;
        }

        Dispatcher.UIThread.Post(
            () => this.FindControl<Control>(targetName)?.Focus(),
            DispatcherPriority.Background);
    }

    private static string GetFocusTargetName(int index) =>
        index switch
        {
            1 => "LookupOperatorCallsignBox",
            2 => "QslSentStatusBox",
            3 => "ContestIdBox",
            4 => "StationCallsignSnapshotBox",
            5 => "ExtraFieldsBox",
            _ => "WorkedCallsignBox"
        };
}
