using System.Collections.Generic;
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
    private readonly DispatcherTimer _rigTimer;
    private readonly DispatcherTimer _spaceWeatherTimer;
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

    [ObservableProperty]
    private bool _isRigEnabled;

    [ObservableProperty]
    private string _rigStatusText = "Rig: OFF";

    [ObservableProperty]
    private bool _isSpaceWeatherVisible;

    [ObservableProperty]
    private string _spaceWeatherText = string.Empty;

    [ObservableProperty]
    private bool _isHelpOpen;

    [ObservableProperty]
    private HelpOverlayViewModel? _helpOverlay;

    [ObservableProperty]
    private bool _isFullQsoCardOpen;

    [ObservableProperty]
    private FullQsoCardViewModel? _fullQsoCard;

    [ObservableProperty]
    private bool _isLoggerFocused;

    internal MainWindowViewModel(string endpoint)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(endpoint);

        _engine = new EngineGrpcService(GrpcChannel.ForAddress(endpoint));
        RecentQsos = new RecentQsoListViewModel(_engine);
        RecentQsos.PropertyChanged += OnRecentQsosPropertyChanged;
        Logger = new QsoLoggerViewModel(_engine);
        Logger.QsoLogged += OnQsoLogged;
        Logger.LoggerFocusRequested += OnLoggerFocusRequested;
        UpdateUtcClock();
        _utcTimer = CreateUtcTimer();
        _rigTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        _rigTimer.Tick += OnRigTimerTick;
        _spaceWeatherTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(5) };
        _spaceWeatherTimer.Tick += OnSpaceWeatherTimerTick;
    }

    internal MainWindowViewModel(IEngineClient engine)
    {
        _engine = engine;
        RecentQsos = new RecentQsoListViewModel(engine);
        RecentQsos.PropertyChanged += OnRecentQsosPropertyChanged;
        Logger = new QsoLoggerViewModel(engine);
        Logger.QsoLogged += OnQsoLogged;
        Logger.LoggerFocusRequested += OnLoggerFocusRequested;
        UpdateUtcClock();
        _utcTimer = CreateUtcTimer();
        _rigTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        _rigTimer.Tick += OnRigTimerTick;
        _spaceWeatherTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(5) };
        _spaceWeatherTimer.Tick += OnSpaceWeatherTimerTick;
    }

    public RecentQsoListViewModel RecentQsos { get; }

    public QsoLoggerViewModel Logger { get; }

    /// <summary>
    /// Proxy for <see cref="RecentQsoListViewModel.SelectedQso"/> so the Inspector
    /// panel can bind via a single-level property path from the window DataContext.
    /// </summary>
    public RecentQsoItemViewModel? InspectorQso => RecentQsos.SelectedQso;

    public bool HasInspectorQso => InspectorQso is not null;

    public event EventHandler? SearchFocusRequested;

    public event EventHandler? GridFocusRequested;

    public event EventHandler? LoggerFocusRequested;

    /// <summary>
    /// Raised when the user requests the Settings dialog. The View subscribes to
    /// this event and opens the modal <see cref="Views.SettingsView"/>.
    /// </summary>
    internal event EventHandler? SettingsRequested;

    /// <summary>
    /// Called after the main window has loaded. Checks first-run state.
    /// </summary>
    public async Task CheckFirstRunAsync(bool focusSearch = false)
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
    private void FocusLogger()
    {
        if (!IsWizardOpen)
        {
            CloseTransientPanels();
            Logger.FocusLogger();
        }
    }

    [RelayCommand]
    private void FocusGrid()
    {
        if (!IsWizardOpen)
        {
            CloseTransientPanels();
            GridFocusRequested?.Invoke(this, EventArgs.Empty);
        }
    }

    [RelayCommand]
    private void ToggleRigControl()
    {
        IsRigEnabled = !IsRigEnabled;
        if (IsRigEnabled)
        {
            RigStatusText = "Rig: connecting\u2026";
            _rigTimer.Start();
        }
        else
        {
            _rigTimer.Stop();
            RigStatusText = "Rig: OFF";
        }
    }

    [RelayCommand]
    private void ToggleSpaceWeather()
    {
        IsSpaceWeatherVisible = !IsSpaceWeatherVisible;
        if (IsSpaceWeatherVisible && string.IsNullOrEmpty(SpaceWeatherText))
        {
            _ = FetchSpaceWeatherAsync();
        }
    }

    [RelayCommand]
    private void ToggleHelp()
    {
        if (IsHelpOpen)
        {
            CloseHelp();
            return;
        }

        var vm = new HelpOverlayViewModel();
        vm.CloseRequested += OnHelpCloseRequested;
        HelpOverlay = vm;
        IsHelpOpen = true;
    }

    [RelayCommand]
    private void ToggleFullQsoCard()
    {
        if (IsFullQsoCardOpen)
        {
            CloseFullQsoCard();
            return;
        }

        var vm = new FullQsoCardViewModel(Logger);
        vm.CloseRequested += OnFullQsoCardCloseRequested;
        FullQsoCard = vm;
        IsFullQsoCardOpen = true;
    }

    [RelayCommand]
    private void ToggleInspector()
    {
        IsInspectorOpen = !IsInspectorOpen;
        if (!IsInspectorOpen)
        {
            GridFocusRequested?.Invoke(this, EventArgs.Empty);
        }
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
        if (IsCallsignCardOpen)
        {
            CloseCallsignCard();
            return;
        }

        // If logger has focus and has a callsign, use that
        string? callsign = null;
        if (IsLoggerFocused && !string.IsNullOrWhiteSpace(Logger.Callsign))
        {
            callsign = Logger.Callsign.Trim().ToUpperInvariant();
        }
        else
        {
            var selectedQso = RecentQsos.SelectedQso;
            if (selectedQso is not null && !string.IsNullOrWhiteSpace(selectedQso.WorkedCallsign))
            {
                callsign = selectedQso.WorkedCallsign;
            }
        }

        if (string.IsNullOrWhiteSpace(callsign))
        {
            return;
        }

        var vm = new CallsignCardViewModel(_engine);
        vm.CloseRequested += OnCallsignCardCloseRequested;
        CallsignCard = vm;
        IsCallsignCardOpen = true;
        _ = vm.LoadAsync(callsign);
    }

    [RelayCommand]
    private void CloseCallsignCard()
    {
        if (CallsignCard is { } card)
        {
            card.CloseRequested -= OnCallsignCardCloseRequested;
        }

        var wasLoggerFocused = IsLoggerFocused;
        IsCallsignCardOpen = false;
        CallsignCard = null;

        if (wasLoggerFocused)
        {
            Logger.FocusLogger();
        }
        else
        {
            GridFocusRequested?.Invoke(this, EventArgs.Empty);
        }
    }

    private void OnCallsignCardCloseRequested(object? sender, EventArgs e)
    {
        CloseCallsignCard();
    }

    private void CloseHelp()
    {
        if (HelpOverlay is { } h)
        {
            h.CloseRequested -= OnHelpCloseRequested;
        }

        IsHelpOpen = false;
        HelpOverlay = null;
        GridFocusRequested?.Invoke(this, EventArgs.Empty);
    }

    private void OnHelpCloseRequested(object? sender, EventArgs e)
    {
        CloseHelp();
    }

    private void CloseFullQsoCard()
    {
        if (FullQsoCard is { } card)
        {
            card.CloseRequested -= OnFullQsoCardCloseRequested;
        }

        IsFullQsoCardOpen = false;
        FullQsoCard = null;
        Logger.FocusLogger();
    }

    private void OnFullQsoCardCloseRequested(object? sender, EventArgs e)
    {
        CloseFullQsoCard();
    }

    private async void OnQsoLogged(object? sender, EventArgs e)
    {
        await RecentQsos.RefreshAsync();
    }

    private void OnLoggerFocusRequested(object? sender, EventArgs e)
    {
        LoggerFocusRequested?.Invoke(this, EventArgs.Empty);
    }

    private async void OnRigTimerTick(object? sender, EventArgs e)
    {
        try
        {
            var response = await _engine.GetRigSnapshotAsync();
            if (response.Snapshot is { } snapshot)
            {
                if (snapshot.Status == QsoRipper.Domain.RigConnectionStatus.Connected)
                {
                    var freqMhz = snapshot.FrequencyHz / 1_000_000.0;
                    RigStatusText = $"Rig: {freqMhz.ToString("F3", CultureInfo.InvariantCulture)} {snapshot.Mode}";
                    Logger.ApplyRigSnapshot(snapshot);
                }
                else
                {
                    RigStatusText = $"Rig: {snapshot.Status}";
                }
            }
        }
        catch (Grpc.Core.RpcException)
        {
            RigStatusText = "Rig: error";
        }
    }

    private async void OnSpaceWeatherTimerTick(object? sender, EventArgs e)
    {
        await FetchSpaceWeatherAsync();
    }

    private async Task FetchSpaceWeatherAsync()
    {
        try
        {
            var response = await _engine.GetCurrentSpaceWeatherAsync();
            if (response.Snapshot is { } sw && sw.Status == QsoRipper.Domain.SpaceWeatherStatus.Current)
            {
                var parts = new List<string>();
                if (sw.HasPlanetaryKIndex)
                {
                    parts.Add($"K:{sw.PlanetaryKIndex.ToString("F0", CultureInfo.InvariantCulture)}");
                }

                if (sw.HasSolarFluxIndex)
                {
                    parts.Add($"SFI:{sw.SolarFluxIndex.ToString("F0", CultureInfo.InvariantCulture)}");
                }

                if (sw.HasSunspotNumber)
                {
                    parts.Add($"SN:{sw.SunspotNumber.ToString(CultureInfo.InvariantCulture)}");
                }

                SpaceWeatherText = parts.Count > 0 ? string.Join(" ", parts) : "Weather: no data";
            }
            else
            {
                SpaceWeatherText = "Weather: unavailable";
            }
        }
        catch (Grpc.Core.RpcException)
        {
            SpaceWeatherText = "Weather: error";
        }
    }

    [RelayCommand]
    private void CloseTransientPanels()
    {
        IsSortChooserOpen = false;
        IsColumnChooserOpen = false;
        CloseCallsignCard();
        GridFocusRequested?.Invoke(this, EventArgs.Empty);
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

        _rigTimer.Stop();
        _rigTimer.Tick -= OnRigTimerTick;

        _spaceWeatherTimer.Stop();
        _spaceWeatherTimer.Tick -= OnSpaceWeatherTimerTick;

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

        _ = FetchSpaceWeatherAsync();
        _spaceWeatherTimer.Start();

        if (IsWizardOpen)
        {
            return;
        }

        if (focusSearch)
        {
            SearchFocusRequested?.Invoke(this, EventArgs.Empty);
        }
        else
        {
            // Default: focus the QSO logger callsign field for immediate entry
            Logger.FocusLogger();
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

    private void OnRecentQsosPropertyChanged(object? sender, System.ComponentModel.PropertyChangedEventArgs e)
    {
        if (e.PropertyName == nameof(RecentQsoListViewModel.SelectedQso))
        {
            OnPropertyChanged(nameof(InspectorQso));
            OnPropertyChanged(nameof(HasInspectorQso));
        }
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
