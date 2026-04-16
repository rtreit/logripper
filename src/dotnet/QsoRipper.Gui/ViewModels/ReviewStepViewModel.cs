using System.Collections.Generic;
using System.Globalization;
using System.Linq;
using System.Text;
using CommunityToolkit.Mvvm.ComponentModel;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class ReviewStepViewModel : WizardStepViewModel
{
    public override string Title => "Review";
    public override string Description => "Review your settings and save.";

    [ObservableProperty]
    private string _summaryText = string.Empty;

    public override Dictionary<string, string> GetFields() => [];

    public void UpdateSummary(IList<WizardStepViewModel> priorSteps)
    {
        var sb = new StringBuilder();

        foreach (var step in priorSteps)
        {
            sb.Append(CultureInfo.InvariantCulture, $"── {step.Title} ──").AppendLine();
            foreach (var kvp in step.GetFields().Where(f => !string.IsNullOrWhiteSpace(f.Value)))
            {
                sb.Append(CultureInfo.InvariantCulture, $"  {FormatFieldName(kvp.Key)}: {MaskIfSensitive(kvp.Key, kvp.Value)}").AppendLine();
            }

            sb.AppendLine();
        }

        SummaryText = sb.ToString().TrimEnd();
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

        return CultureInfo.InvariantCulture.TextInfo.ToTitleCase(upper.ToLowerInvariant());
    }

    private static string MaskIfSensitive(string key, string value)
    {
        if ((key.Contains("password", StringComparison.OrdinalIgnoreCase)
            || key.Contains("api_key", StringComparison.OrdinalIgnoreCase))
            && value.Length > 0)
        {
            return new string('•', value.Length);
        }

        return value;
    }
}
