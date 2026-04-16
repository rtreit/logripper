using System.Collections.Generic;
using CommunityToolkit.Mvvm.ComponentModel;
using Google.Protobuf.Collections;
using QsoRipper.Services;

namespace QsoRipper.Gui.ViewModels;

/// <summary>
/// Base class for each wizard step. Provides common state and error display.
/// </summary>
internal abstract partial class WizardStepViewModel : ObservableObject
{
    public abstract string Title { get; }
    public abstract string Description { get; }
    public virtual bool IsSkippable => false;

    [ObservableProperty]
    private bool _isComplete;

    [ObservableProperty]
    private string? _validationSummary;

    /// <summary>
    /// Returns the field key-value pairs for this step, used by ValidateSetupStep RPC.
    /// </summary>
    public abstract Dictionary<string, string> GetFields();

    /// <summary>
    /// Applies field-level validation errors returned from the engine.
    /// </summary>
    public virtual void ApplyValidationErrors(RepeatedField<SetupFieldValidation> validations)
    {
        var errors = new List<string>();
        foreach (var v in validations)
        {
            if (!v.Valid)
            {
                errors.Add($"{FormatFieldName(v.Field)}: {v.Message}");
            }
        }

        ValidationSummary = errors.Count > 0 ? string.Join("\n", errors) : null;
    }

    public virtual void ClearErrors()
    {
        ValidationSummary = null;
    }

    private static string FormatFieldName(string key)
    {
        var upper = key
            .Replace('_', ' ')
            .Replace('.', ' ')
            .ToUpperInvariant();
        if (upper.StartsWith("QRZ", StringComparison.Ordinal))
        {
            return upper;
        }

        return System.Globalization.CultureInfo.InvariantCulture.TextInfo.ToTitleCase(upper.ToLowerInvariant());
    }
}
