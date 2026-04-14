using System.Collections.Generic;
using System.Globalization;
using CommunityToolkit.Mvvm.ComponentModel;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class QrzLogbookStepViewModel : WizardStepViewModel
{
    public override string Title => "QRZ Logbook (optional)";

    public override string Description =>
        "Configure your QRZ.com Logbook API key for bidirectional sync.";

    public override bool IsSkippable => true;

    [ObservableProperty]
    private string? _apiKey;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsSyncIntervalEnabled))]
    private bool _autoSyncEnabled;

    [ObservableProperty]
    private int _syncIntervalSeconds = 300;

    public bool IsSyncIntervalEnabled => AutoSyncEnabled;

    /// <summary>
    /// True when a key was already configured on the server (we never receive the actual secret).
    /// </summary>
    [ObservableProperty]
    private bool _hasExistingKey;

    public override Dictionary<string, string> GetFields()
    {
        return new Dictionary<string, string>
        {
            ["qrz_logbook_api_key"] = HasExistingKey && string.IsNullOrWhiteSpace(ApiKey)
                ? "(configured)"
                : ApiKey ?? string.Empty,
            ["auto_sync_enabled"] = AutoSyncEnabled.ToString(CultureInfo.InvariantCulture),
            ["sync_interval_seconds"] = SyncIntervalSeconds.ToString(CultureInfo.InvariantCulture),
        };
    }

    /// <summary>
    /// Client-side validation for the QRZ Logbook step.
    /// Returns <see langword="true"/> when the step is valid.
    /// </summary>
    public bool ValidateLocally()
    {
        if (AutoSyncEnabled && SyncIntervalSeconds is < 60 or > 3600)
        {
            ValidationSummary = "Sync interval must be between 60 and 3600 seconds.";
            return false;
        }

        ClearErrors();
        return true;
    }
}
