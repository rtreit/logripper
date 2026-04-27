using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class HelpOverlayViewModel : ObservableObject
{
    internal record ShortcutEntry(string Key, string Description);

    internal record ShortcutGroup(string Title, ShortcutEntry[] Entries);

    public ShortcutGroup[] Groups { get; } =
    [
        new("Navigation", [
            new("F1", "Toggle help"),
            new("F3", "Focus QSO grid"),
            new("F4 / Ctrl+F", "Focus search"),
            new("Ctrl+N", "Focus QSO logger"),
            new("Tab / Shift+Tab", "Cycle logger fields"),
            new("Esc", "Close overlay / clear"),
        ]),
        new("QSO Logging", [
            new("Ctrl+Enter / F10", "Log QSO"),
            new("Alt+A / Ctrl+L", "Open QSO Card"),
            new("Alt+C", "Jump to callsign"),
            new("Alt+B", "Jump to band"),
            new("Alt+M", "Jump to mode"),
            new("F7", "Start QSO timer"),
            new("\u2190 / \u2192", "Cycle band/mode (when focused)"),
        ]),
        new("QSO Card", [
            new("Alt+1\u20266", "Jump to tab (Core\u2026Metadata)"),
            new("Ctrl+Tab", "Next tab"),
            new("Ctrl+Shift+Tab", "Previous tab"),
            new("Ctrl+Enter / Ctrl+S", "Save"),
            new("Esc", "Close card"),
        ]),
        new("Grid", [
            new("F2", "Edit selected cell"),
            new("Ctrl+S", "Save edits"),
            new("Ctrl+D / Delete", "Delete selected QSO"),
            new("F5", "Refresh"),
            new("F8", "Callsign card"),
            new("F9", "CW stats pane"),
            new("Alt+Enter", "Toggle inspector"),
            new("Ctrl++ / Ctrl+-", "Zoom in / out"),
            new("Ctrl+0", "Reset zoom"),
        ]),
        new("System", [
            new("F6", "Sync with QRZ"),
            new("Ctrl+R", "Toggle rig control"),
            new("Ctrl+W", "Toggle space weather"),
            new("Ctrl+,", "Settings"),
            new("Ctrl+Shift+S", "Sort chooser"),
            new("Ctrl+H", "Column chooser"),
            new("Ctrl+Q / Alt+X", "Quit"),
        ]),
    ];

    public event EventHandler? CloseRequested;

    [RelayCommand]
    private void Close()
    {
        CloseRequested?.Invoke(this, EventArgs.Empty);
    }
}
