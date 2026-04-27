using System.ComponentModel;
using System.Runtime.CompilerServices;

namespace CwDecoderGui.Models;

public sealed class SelectableLabelFile : INotifyPropertyChanged
{
    private bool _isSelected;

    public SelectableLabelFile(string path, string? displayName = null)
    {
        Path = path;
        DisplayName = displayName ?? System.IO.Path.GetFileName(path);
    }

    public string Path { get; }
    public string DisplayName { get; }

    public bool IsSelected
    {
        get => _isSelected;
        set
        {
            if (_isSelected == value)
            {
                return;
            }

            _isSelected = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(IsSelected)));
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;
}
