using QsoRipper.Services;

namespace QsoRipper.DebugHost.Models;

internal sealed class RuntimeConfigEditorEntry
{
    public RuntimeConfigEditorEntry(RuntimeConfigDefinition definition, RuntimeConfigValue? currentValue)
    {
        ArgumentNullException.ThrowIfNull(definition);

        Definition = definition;
        UpdateCurrentValue(currentValue);
    }

    public RuntimeConfigDefinition Definition { get; }

    public RuntimeConfigValue? CurrentValue { get; private set; }

    public string DraftValue { get; set; } = string.Empty;

    public string EffectiveDisplayValue => CurrentValue is { HasValue: true }
        ? CurrentValue.DisplayValue
        : "(unset)";

    public bool IsOverridden => CurrentValue?.Overridden ?? false;

    public bool IsSecret => Definition.Secret;

    public bool HasAllowedValues => Definition.AllowedValues.Count > 0;

    public bool CanSave => HasAllowedValues || !string.IsNullOrWhiteSpace(DraftValue);

    public void UpdateCurrentValue(RuntimeConfigValue? currentValue)
    {
        CurrentValue = currentValue;

        if (IsSecret)
        {
            DraftValue = string.Empty;
            return;
        }

        if (currentValue is { HasValue: true })
        {
            DraftValue = currentValue.DisplayValue;
        }
        else if (HasAllowedValues)
        {
            DraftValue = Definition.AllowedValues[0];
        }
        else if (!HasAllowedValues)
        {
            DraftValue = string.Empty;
        }
    }

    public string GetMutationValue()
    {
        return DraftValue.Trim();
    }
}
