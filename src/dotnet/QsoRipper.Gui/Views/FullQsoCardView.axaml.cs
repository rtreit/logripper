using Avalonia;
using Avalonia.Controls;
using Avalonia.Input;
using Avalonia.Threading;

namespace QsoRipper.Gui.Views;

internal sealed partial class FullQsoCardView : UserControl
{
    public FullQsoCardView()
    {
        InitializeComponent();
    }

    protected override void OnAttachedToVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnAttachedToVisualTree(e);
        Dispatcher.UIThread.Post(FocusInitialField, DispatcherPriority.Loaded);
    }

    internal void FocusInitialField() => FocusWorkedCallsign();

    internal bool TryHandleNavigationKey(Key key, KeyModifiers modifiers)
    {
        var currentIndex = this.FindControl<TabControl>("CardTabs")?.SelectedIndex ?? 0;
        if (!FullQsoCardNavigation.TryResolve(key, modifiers, currentIndex, out var targetIndex))
        {
            return false;
        }

        FocusTab(targetIndex, GetFocusTargetName(targetIndex));
        return true;
    }

    protected override void OnKeyDown(KeyEventArgs e)
    {
        if (TryHandleNavigationKey(e.Key, e.KeyModifiers))
        {
            e.Handled = true;
            return;
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
            FocusWorkedCallsign();
            return;
        }

        FocusTab(viewModel.SelectedTabIndex, viewModel.SelectedTabIndex switch
        {
            _ => GetFocusTargetName(viewModel.SelectedTabIndex)
        });
    }

    internal void FocusWorkedCallsign() => FocusTab(0, "WorkedCallsignBox");

    private void FocusTab(int index, string targetName)
    {
        if (this.FindControl<TabControl>("CardTabs") is { } tabs)
        {
            tabs.SelectedIndex = index;
        }

        Dispatcher.UIThread.Post(
            () => this.FindControl<Control>(targetName)?.Focus(),
            DispatcherPriority.Loaded);
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
