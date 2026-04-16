using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using CommunityToolkit.Mvvm.ComponentModel;
using QsoRipper.Services;
using QsoRipper.Shared.Persistence;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class LogFileStepViewModel : WizardStepViewModel
{
    private string _description = "Where should QsoRipper store persisted logbook data?";
    private string _title = "Storage";

    public override string Title => _title;

    public override string Description => _description;

    public ObservableCollection<PersistenceSetupField> PersistenceFields { get; } = [];

    public bool HasPersistenceInputs => PersistenceFields.Count > 0;

    public bool ShowsInfoOnly => !HasPersistenceInputs;

    public override Dictionary<string, string> GetFields()
    {
        if (PersistenceFields.Count == 0)
        {
            return [];
        }

        return PersistenceFields
            .Where(field => !string.IsNullOrWhiteSpace(field.Value))
            .ToDictionary(
                field => string.IsNullOrWhiteSpace(field.Label) ? field.Key : field.Label,
                field => field.Value.Trim(),
                StringComparer.OrdinalIgnoreCase);
    }

    public void ConfigureFromSetupStatus(SetupStatus status)
    {
        ArgumentNullException.ThrowIfNull(status);

        _title = string.IsNullOrWhiteSpace(status.PersistenceLabel)
            ? "Storage"
            : status.PersistenceLabel;
        _description = string.IsNullOrWhiteSpace(status.PersistenceDescription)
            ? "Where should QsoRipper store persisted logbook data?"
            : status.PersistenceDescription;
        ReplacePersistenceFields(PersistenceSetupFields.FromStatus(status, status.SuggestedLogFilePath ?? string.Empty));

        OnPropertyChanged(nameof(Title));
        OnPropertyChanged(nameof(Description));
        OnPropertyChanged(nameof(HasPersistenceInputs));
        OnPropertyChanged(nameof(ShowsInfoOnly));
    }

    private void ReplacePersistenceFields(IEnumerable<PersistenceSetupField> fields)
    {
        PersistenceFields.Clear();
        foreach (var field in fields)
        {
            PersistenceFields.Add(field);
        }
    }
}
