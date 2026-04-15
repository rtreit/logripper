using System.Globalization;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Grpc.Net.Client;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class MainWindowViewModel : ObservableObject, IDisposable
{
    private readonly IEngineClient _engine;
    private readonly DispatcherTimer _utcTimer;
    private bool _setupCompleteBeforeWizard;

    [ObservableProperty]
    private bool _isSettingsOpen;

    [ObservableProperty]
    private bool _isWizardOpen;

    [ObservableProperty]
    private SetupWizardViewModel? _wizardViewModel;

    [ObservableProperty]
    private string _statusMessage = "Checking engine connection...";

    [ObservableProperty]
    private bool _isSetupIncomplete;

    [ObservableProperty]
    private string _activeLogText = "Log: -";

    [ObservableProperty]
    private string _activeProfileText = "Profile: -";

    [ObservableProperty]
    private string _activeStationText = "Station: -";

    [ObservableProperty]
    private bool _isInspectorOpen;

    [ObservableProperty]
    private bool _isSyncing;

    [ObservableProperty]
    private string _syncStatusText = "Sync: never";

    [ObservableProperty]
    private bool _isSortChooserOpen;

    [ObservableProperty]
    private bool _isColumnChooserOpen;

    [ObservableProperty]
    private string _currentUtcTime = string.Empty;

    [ObservableProperty]
    private string _currentUtcDate = string.Empty;

    [ObservableProperty]
    private bool _isCallsignCardOpen;

    [ObservableProperty]
    private CallsignCardViewModel? _callsignCard;

    internal MainWindowViewModel(string endpoint)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(endpoint);

        _engine = new EngineGrpcService(GrpcChannel.ForAddress(endpoint));
        RecentQsos = new RecentQsoListViewModel(_engine);
        UpdateUtcClock();
        _utcTimer = CreateUtcTimer();
    }

    internal MainWindowViewModel(IEngineClient engine)
    {
        _engine = engine;
        RecentQsos = new RecentQsoListViewModel(engine);
        UpdateUtcClock();
        _utcTimer = CreateUtcTimer();
    }

    public RecentQsoListViewModel RecentQsos { get; }

    public event EventHandler? SearchFocusRequested;

    /// <summary>
    /// Raised when the user requests the Settings dialog. The View subscribes to
    /// this event and opens the modal <see cref="Views.SettingsView"/>.
    /// </summary>
    internal event EventHandler? SettingsRequested;

    /// <summary>
    /// Called after the main window has loaded. Checks first-run state.
    /// </summary>
    public async Task CheckFirstRunAsync(bool focusSearch = true)
    {
        try
        {
            var state = await _engine.GetWizardStateAsync();
            ApplySetupContext(state);
            IsSetupIncomplete = !state.Status.SetupComplete;

            if (state.Status.IsFirstRun)
            {
                StatusMessage = "Welcome";
                await OpenWizardAsync();
            }
            else
            {
                await ActivateDashboardAsync(focusSearch);
            }
        }
        catch (Grpc.Core.RpcException)
        {
            StatusMessage = "Engine unavailable";
        }
    }

    [RelayCommand]
    private async Task OpenWizardAsync()
    {
        _setupCompleteBeforeWizard = !IsSetupIncomplete;
        var vm = new SetupWizardViewModel(_engine, this);
        WizardViewModel = vm;
        IsWizardOpen = true;
        await vm.LoadStateAsync();
    }

    [RelayCommand]
    private void OpenSettings()
    {
        if (!IsWizardOpen && !IsSettingsOpen)
        {
            SettingsRequested?.Invoke(this, EventArgs.Empty);
        }
    }

    [RelayCommand(CanExecute = nameof(CanSyncNow))]
    private async Task SyncNowAsync()
    {
        IsSyncing = true;
        SyncStatusText = "Syncing\u2026";
        try
        {
            var response = await _engine.SyncWithQrzAsync();

            if (!string.IsNullOrEmpty(response.Error))
            {
                SyncStatusText = $"Sync error: {response.Error}";
                return;
            }

            var up = response.UploadedRecords;
            var down = response.DownloadedRecords;
            SyncStatusText = $"Synced: \u2191{up} \u2193{down}";
            await RecentQsos.RefreshAsync();
        }
        catch (Grpc.Core.RpcException ex)
        {
            SyncStatusText = $"Sync failed: {ex.Status.Detail}";
        }
        finally
        {
            IsSyncing = false;
        }
    }

    private bool CanSyncNow() => !IsSyncing && !IsWizardOpen;

    partial void OnIsSyncingChanged(bool value) => SyncNowCommand.NotifyCanExecuteChanged();

    partial void OnIsWizardOpenChanged(bool value)
    {
        SyncNowCommand.NotifyCanExecuteChanged();
    }

    /// <summary>
    /// Creates a <see cref="SettingsViewModel"/> wired to the shared engine client.
    /// Called by the View layer when handling <see cref="SettingsRequested"/>.
    /// </summary>
    internal SettingsViewModel CreateSettingsViewModel() => new(_engine);

    /// <summary>
    /// Called by the View layer after the Settings dialog closes.
    /// </summary>
    internal async Task OnSettingsClosedAsync(bool didSave)
    {
        IsSettingsOpen = false;
        if (didSave)
        {
            await RefreshSetupContextAsync();
            await ActivateDashboardAsync(focusSearch: false);
        }
    }

    [RelayCommand]
    private void FocusSearch()
    {
        if (!IsWizardOpen)
        {
            CloseTransientPanels();
            SearchFocusRequested?.Invoke(this, EventArgs.Empty);
        }
    }

    [RelayCommand]
    private void ToggleInspector()
    {
        IsInspectorOpen = !IsInspectorOpen;
    }

    [RelayCommand]
    private void ToggleSortChooser()
    {
        IsSortChooserOpen = !IsSortChooserOpen;
        if (IsSortChooserOpen)
        {
            IsColumnChooserOpen = false;
        }
    }

    [RelayCommand]
    private void ToggleColumnChooser()
    {
        IsColumnChooserOpen = !IsColumnChooserOpen;
        if (IsColumnChooserOpen)
        {
            IsSortChooserOpen = false;
        }
    }

    [RelayCommand]
    private void OpenCallsignCard()
    {
        var selectedQso = RecentQsos.SelectedQso;
        if (selectedQso is null || string.IsNullOrWhiteSpace(selectedQso.WorkedCallsign))
        {
            return;
        }

        var vm = new CallsignCardViewModel(_engine);
        vm.CloseRequested += OnCallsignCardCloseRequested;
        CallsignCard = vm;
        IsCallsignCardOpen = true;
        _ = vm.LoadAsync(selectedQso.WorkedCallsign);
    }

    [RelayCommand]
    private void CloseCallsignCard()
    {
        if (CallsignCard is { } card)
        {
            card.CloseRequested -= OnCallsignCardCloseRequested;
        }

        IsCallsignCardOpen = false;
        CallsignCard = null;
    }

    private void OnCallsignCardCloseRequested(object? sender, EventArgs e)
    {
        CloseCallsignCard();
    }

    [RelayCommand]
    private void CloseTransientPanels()
    {
        IsSortChooserOpen = false;
        IsColumnChooserOpen = false;
        CloseCallsignCard();
    }

    [RelayCommand]
    private static void Exit()
    {
        if (Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime lifetime)
        {
            lifetime.Shutdown();
        }
    }

    internal void CancelWizard()
    {
        IsWizardOpen = false;
        WizardViewModel = null;
        IsSetupIncomplete = !_setupCompleteBeforeWizard;
        StatusMessage = _setupCompleteBeforeWizard
            ? "Ready"
            : "Setup incomplete";

        if (_setupCompleteBeforeWizard)
        {
            _ = ActivateDashboardAsync(focusSearch: true);
        }
    }

    internal void CloseWizard(bool setupComplete)
    {
        IsWizardOpen = false;
        WizardViewModel = null;
        IsSetupIncomplete = !setupComplete;
        StatusMessage = setupComplete
            ? "Ready"
            : "Setup incomplete";

        _ = RefreshSetupContextAsync();

        if (setupComplete)
        {
            _ = ActivateDashboardAsync(focusSearch: true);
        }
    }

    public void Dispose()
    {
        _utcTimer.Stop();
        _utcTimer.Tick -= UtcTimerOnTick;

        if (_engine is IDisposable disposable)
        {
            disposable.Dispose();
        }
    }

    private async Task ActivateDashboardAsync(bool focusSearch)
    {
        StatusMessage = "Ready";
        await RecentQsos.RefreshAsync();
        await RefreshSyncStatusAsync();

        if (focusSearch && !IsWizardOpen)
        {
            SearchFocusRequested?.Invoke(this, EventArgs.Empty);
        }
    }

    private void UtcTimerOnTick(object? sender, EventArgs e)
    {
        UpdateUtcClock();
    }

    private void UpdateUtcClock()
    {
        var utcNow = DateTimeOffset.UtcNow;
        CurrentUtcTime = utcNow.ToString("HH:mm:ss 'UTC'", CultureInfo.InvariantCulture);
        CurrentUtcDate = utcNow.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture);
    }

    private DispatcherTimer CreateUtcTimer()
    {
        var timer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(1)
        };
        timer.Tick += UtcTimerOnTick;
        timer.Start();
        return timer;
    }

    private async Task RefreshSetupContextAsync()
    {
        try
        {
            ApplySetupContext(await _engine.GetWizardStateAsync());
        }
        catch (Grpc.Core.RpcException)
        {
            StatusMessage = "Engine unavailable";
        }
    }

    private void ApplySetupContext(QsoRipper.Services.GetSetupWizardStateResponse state)
    {
        var activeProfile = state.StationProfiles.FirstOrDefault(profile => profile.IsActive)?.Profile
            ?? state.Status.StationProfile;
        ActiveLogText = BuildLogText(state.Status.LogFilePath);
        ActiveProfileText = BuildProfileText(activeProfile);
        ActiveStationText = BuildStationText(activeProfile);
    }

    private static string BuildLogText(string? logFilePath)
    {
        if (string.IsNullOrWhiteSpace(logFilePath))
        {
            return "Log: -";
        }

        return $"Log: {Path.GetFileNameWithoutExtension(logFilePath.Trim())}";
    }

    private static string BuildProfileText(QsoRipper.Domain.StationProfile? profile)
    {
        var profileName = profile?.ProfileName;
        return string.IsNullOrWhiteSpace(profileName)
            ? "Profile: Default"
            : $"Profile: {profileName.Trim()}";
    }

    private static string BuildStationText(QsoRipper.Domain.StationProfile? profile)
    {
        var stationCallsign = profile?.StationCallsign;
        return string.IsNullOrWhiteSpace(stationCallsign)
            ? "Station: -"
            : $"Station: {stationCallsign.Trim()}";
    }

    private async Task RefreshSyncStatusAsync()
    {
        try
        {
            var status = await _engine.GetSyncStatusAsync();
            if (status.IsSyncing)
            {
                SyncStatusText = "Syncing\u2026";
            }
            else if (status.LastSync is not null)
            {
                var elapsed = DateTimeOffset.UtcNow - status.LastSync.ToDateTimeOffset();
                SyncStatusText = elapsed.TotalMinutes < 1
                    ? "Last sync: just now"
                    : elapsed.TotalHours < 1
                        ? $"Last sync: {(int)elapsed.TotalMinutes}m ago"
                        : elapsed.TotalDays < 1
                            ? $"Last sync: {(int)elapsed.TotalHours}h ago"
                            : $"Last sync: {(int)elapsed.TotalDays}d ago";
            }
            else
            {
                SyncStatusText = "Sync: never";
            }
        }
        catch (Grpc.Core.RpcException)
        {
            // Sync status unavailable — leave current text unchanged.
        }
    }
}
