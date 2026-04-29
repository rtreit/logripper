using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace CwDecoderGui.Models;

/// <summary>
/// One row in the TUNING tab strategy picker. Backs a checkbox plus tooltip;
/// the resolved set of checked tokens (plus operator-supplied custom tokens)
/// is forwarded to the Rust eval binary by RunStrategySweepAsync.
/// </summary>
public sealed class StrategyOption : INotifyPropertyChanged
{
    public StrategyOption(string token, string label, bool defaultChecked, string tooltip)
    {
        Token = token;
        Label = label;
        DefaultChecked = defaultChecked;
        Tooltip = tooltip;
        _isChecked = defaultChecked;
    }

    /// <summary>Token forwarded verbatim to the eval binary (e.g. "auto", "28", "region28").</summary>
    public string Token { get; }

    /// <summary>Display label shown next to the checkbox.</summary>
    public string Label { get; }

    /// <summary>Tooltip explaining what this strategy does.</summary>
    public string Tooltip { get; }

    /// <summary>Documented default checked state, used by RESET TO DEFAULTS.</summary>
    public bool DefaultChecked { get; }

    private bool _isChecked;
    public bool IsChecked
    {
        get => _isChecked;
        set
        {
            if (_isChecked == value) return;
            _isChecked = value;
            OnPropertyChanged();
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private void OnPropertyChanged([CallerMemberName] string? name = null)
        => PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(name!));
}
