using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class FullQsoCardViewModel : ObservableObject
{
    private readonly QsoLoggerViewModel _logger;

    public FullQsoCardViewModel(QsoLoggerViewModel logger)
    {
        _logger = logger;
    }

    /// <summary>
    /// Proxy to the logger's core fields so the card can bind them directly.
    /// </summary>
    public QsoLoggerViewModel Logger => _logger;

    // Additional fields for full entry beyond the compact logger.

    [ObservableProperty]
    private string _gridSquare = string.Empty;

    [ObservableProperty]
    private string _name = string.Empty;

    [ObservableProperty]
    private string _country = string.Empty;

    [ObservableProperty]
    private string _state = string.Empty;

    public event EventHandler? CloseRequested;

    [RelayCommand]
    private void Close()
    {
        CloseRequested?.Invoke(this, EventArgs.Empty);
    }

    /// <summary>
    /// Populate extra fields from a lookup result.
    /// </summary>
    public void ApplyLookup(string? name, string? grid, string? country)
    {
        if (!string.IsNullOrWhiteSpace(name))
        {
            Name = name;
        }

        if (!string.IsNullOrWhiteSpace(grid))
        {
            GridSquare = grid;
        }

        if (!string.IsNullOrWhiteSpace(country))
        {
            Country = country;
        }
    }
}
