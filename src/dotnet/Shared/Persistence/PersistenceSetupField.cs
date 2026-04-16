using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.CompilerServices;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Shared.Persistence;

internal sealed class PersistenceSetupField : INotifyPropertyChanged
{
    private string _value = string.Empty;

    public string Key { get; init; } = string.Empty;

    public string Label { get; init; } = string.Empty;

    public string Description { get; init; } = string.Empty;

    public RuntimeConfigValueKind Kind { get; init; } = RuntimeConfigValueKind.String;

    public bool Required { get; init; }

    public bool Secret { get; init; }

    public bool HasConfiguredValue { get; init; }

    public bool IsRedacted { get; init; }

    public IReadOnlyList<string> AllowedValues { get; init; } = [];

    public bool HasChoices => AllowedValues.Count > 0;

    public bool IsPath => PersistenceSetup.IsPathKey(Key) || Kind == RuntimeConfigValueKind.Path;

    public string Value
    {
        get => _value;
        set
        {
            var normalized = value ?? string.Empty;
            if (string.Equals(_value, normalized, StringComparison.Ordinal))
            {
                return;
            }

            _value = normalized;
            OnPropertyChanged();
        }
    }

    public event PropertyChangedEventHandler? PropertyChanged;

    private void OnPropertyChanged([CallerMemberName] string? propertyName = null)
    {
        PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(propertyName));
    }
}
