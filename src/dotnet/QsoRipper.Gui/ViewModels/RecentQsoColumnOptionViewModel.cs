using CommunityToolkit.Mvvm.ComponentModel;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class RecentQsoColumnOptionViewModel : ObservableObject
{
    public RecentQsoColumnOptionViewModel(
        RecentQsoGridColumn column,
        string label,
        bool isVisible)
    {
        Column = column;
        Label = label;
        _isVisible = isVisible;
    }

    public RecentQsoGridColumn Column { get; }

    public string Label { get; }

    [ObservableProperty]
    private bool _isVisible;
}
