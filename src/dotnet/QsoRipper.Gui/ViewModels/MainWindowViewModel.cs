using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Gui.Services;
using QsoRipper.Gui.Utilities;
using QsoRipper.Shared.Persistence;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class MainWindowViewModel : ObservableObject, IDisposable
{
    private readonly IEngineClient _engine;
    private readonly SwitchableEngineClient? _switchableEngine;
    private readonly DispatcherTimer _utcTimer;
    private readonly DispatcherTimer _rigTimer;
    private readonly DispatcherTimer _spaceWeatherTimer;
    private bool _setupCompleteBeforeWizard;
    private string? _preferredEngineProfileId;
    private string? _preferredEngineEndpoint;

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
    private string _activeEngineText = "Engine: -";

    [ObservableProperty]
    private string _availableEnginesText = "Engines: unknown";

    [ObservableProperty]
    private string _engineSwitchStatusText = "Switch: idle";

    [ObservableProperty]
    [NotifyCanExecuteChangedFor(nameof(SwitchToRustEngineCommand))]
    [NotifyCanExecuteChangedFor(nameof(SwitchToDotNetEngineCommand))]
    [NotifyCanExecuteChangedFor(nameof(RefreshEngineAvailabilityCommand))]
    private bool _isEngineSwitching;

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

    internal MainWindowViewModel(EngineTargetProfile engineProfile, string endpoint)
    {
        ArgumentNullException.ThrowIfNull(engineProfile);
        ArgumentException.ThrowIfNullOrWhiteSpace(endpoint);

        _switchableEngine = new SwitchableEngineClient(engineProfile, endpoint);
        _engine = _switchableEngine;
        RecentQsos = new RecentQsoListViewModel(_engine);
        RecentQsos.PropertyChanged += OnRecentQsosPropertyChanged;
        Logger = new QsoLoggerViewModel(_engine);
        Logger.QsoLogged += OnQsoLogged;
        Logger.LoggerFocusRequested += OnLoggerFocusRequested;
        ActiveEngineText = BuildEngineText(engineProfile, endpoint);
        UpdateUtcClock();
        _utcTimer = CreateUtcTimer();
        _rigTimer = new DispatcherTimer { Interval = TimeSpan.FromSeconds(1) };
        _rigTimer.Tick += OnRigTimerTick;
        _spaceWeatherTimer = new DispatcherTimer { Interval = TimeSpan.FromMinutes(5) };
        _spaceWeatherTimer.Tick += OnSpaceWeatherTimerTick;
    }

    internal MainWindowViewModel(IEngineClient engine)
    {
        ArgumentNullException.ThrowIfNull(engine);

        _switchableEngine = engine as SwitchableEngineClient;
        _engine = engine;
        RecentQsos = new RecentQsoListViewModel(engine);
        RecentQsos.PropertyChanged += OnRecentQsosPropertyChanged;
        Logger = new QsoLoggerViewModel(engine);
        Logger.QsoLogged += OnQsoLogged;
        Logger.LoggerFocusRequested += OnLoggerFocusRequested;
        if (_switchableEngine is not null)
        {
            ActiveEngineText = BuildEngineText(_switchableEngine.CurrentProfile, _switchableEngine.CurrentEndpoint);
        }
        else
        {
            ActiveEngineText = "Engine: fixture";
            AvailableEnginesText = "Engines: unavailable";
        }

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
            await ApplyPreferredEngineSelectionAsync();
            UpdateAvailableEngineSummary();
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
        SwitchToRustEngineCommand.NotifyCanExecuteChanged();
        SwitchToDotNetEngineCommand.NotifyCanExecuteChanged();
    }

    [RelayCommand(CanExecute = nameof(CanSwitchEngines))]
    private Task SwitchToRustEngineAsync()
    {
        return SwitchEngineProfileAsync(KnownEngineProfiles.LocalRust);
    }

    [RelayCommand(CanExecute = nameof(CanSwitchEngines))]
    private Task SwitchToDotNetEngineAsync()
    {
        return SwitchEngineProfileAsync(KnownEngineProfiles.LocalDotNet);
    }

    [RelayCommand(CanExecute = nameof(CanRefreshEngineAvailability))]
    private void RefreshEngineAvailability()
    {
        UpdateAvailableEngineSummary();
    }

    private bool CanSwitchEngines()
    {
        return _switchableEngine is not null && !IsWizardOpen && !IsEngineSwitching;
    }

    private bool CanRefreshEngineAvailability()
    {
        return _switchableEngine is not null && !IsEngineSwitching;
    }

    private async Task SwitchEngineProfileAsync(string profileId)
    {
        if (_switchableEngine is null)
        {
            return;
        }

        var targetProfile = EngineCatalog.GetProfile(profileId);
        var targetEndpoint = ResolveSwitchEndpoint(targetProfile);
        IsEngineSwitching = true;
        EngineSwitchStatusText = $"Switching to {targetProfile.DisplayName}\u2026";
        try
        {
            using var timeoutSource = new CancellationTokenSource(TimeSpan.FromSeconds(5));
            var result = await _switchableEngine.SwitchAsync(targetProfile, targetEndpoint, timeoutSource.Token);
            EngineSwitchStatusText = string.IsNullOrWhiteSpace(result.Message)
                ? "Switch: ready"
                : result.Message;
            if (!result.Success)
            {
                return;
            }

            ActiveEngineText = BuildEngineText(result.Profile, result.Endpoint);
            await RefreshSetupContextAsync();
            await RecentQsos.RefreshAsync();
            await RefreshSyncStatusAsync();
        }
        finally
        {
            IsEngineSwitching = false;
            UpdateAvailableEngineSummary();
        }
    }

    private async Task ApplyPreferredEngineSelectionAsync()
    {
        if (_switchableEngine is null)
        {
            return;
        }

        if (string.IsNullOrWhiteSpace(_preferredEngineProfileId)
            && string.IsNullOrWhiteSpace(_preferredEngineEndpoint))
        {
            return;
        }

        var targetProfile = EngineCatalog.ResolveProfile(_preferredEngineProfileId);
        var targetEndpoint = EngineCatalog.ResolveEndpoint(targetProfile, _preferredEngineEndpoint);
        if (string.Equals(targetProfile.ProfileId, _switchableEngine.CurrentProfile.ProfileId, StringComparison.OrdinalIgnoreCase)
            && string.Equals(targetEndpoint, _switchableEngine.CurrentEndpoint, StringComparison.OrdinalIgnoreCase))
        {
            _preferredEngineProfileId = null;
            _preferredEngineEndpoint = null;
            return;
        }

        using var timeoutSource = new CancellationTokenSource(TimeSpan.FromSeconds(5));
        var result = await _switchableEngine.SwitchAsync(targetProfile, targetEndpoint, timeoutSource.Token);
        if (result.Success)
        {
            ActiveEngineText = BuildEngineText(result.Profile, result.Endpoint);
        }
        else
        {
            EngineSwitchStatusText = string.IsNullOrWhiteSpace(result.Message)
                ? "Switch: ready"
                : result.Message;
        }

        _preferredEngineProfileId = null;
        _preferredEngineEndpoint = null;
    }

    private void UpdateAvailableEngineSummary()
    {
        if (_switchableEngine is null)
        {
            AvailableEnginesText = "Engines: unavailable";
            return;
        }

        var runtimeEntries = EngineRuntimeDiscovery.DiscoverLocalEngines(new EngineRuntimeDiscoveryOptions
        {
            ValidateTcpReachability = true
        });
        var runningLabels = runtimeEntries
            .Where(static entry => entry.IsRunning)
            .Select(static entry => entry.Profile.DisplayName.Replace("QsoRipper ", string.Empty, StringComparison.Ordinal))
            .ToArray();

        AvailableEnginesText = runningLabels.Length == 0
            ? "Engines: none running"
            : $"Engines: {string.Join(", ", runningLabels)}";
    }

    private static string ResolveSwitchEndpoint(EngineTargetProfile targetProfile)
    {
        var runtimeEntries = EngineRuntimeDiscovery.DiscoverLocalEngines(new EngineRuntimeDiscoveryOptions
        {
            ValidateTcpReachability = true
        });
        var runningEntry = runtimeEntries.FirstOrDefault(entry =>
            entry.IsRunning
            && string.Equals(entry.Profile.ProfileId, targetProfile.ProfileId, StringComparison.OrdinalIgnoreCase));
        return runningEntry?.Endpoint ?? targetProfile.DefaultEndpoint;
    }

    private static string BuildEngineText(EngineTargetProfile profile, string endpoint)
    {
        ArgumentNullException.ThrowIfNull(profile);
        ArgumentException.ThrowIfNullOrWhiteSpace(endpoint);
        return $"Engine: {profile.DisplayName} @ {endpoint.Trim()}";
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

    /// <summary>
    /// Raised when the user requests a column layout reset. The View subscribes
    /// and resets DisplayIndex/width/visibility to XAML defaults.
    /// </summary>
    internal event EventHandler? ColumnLayoutResetRequested;

    [RelayCommand]
    private void ResetColumnLayout()
    {
        ColumnLayoutResetRequested?.Invoke(this, EventArgs.Empty);
        IsColumnChooserOpen = false;
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
        vm.RecordLoaded += OnCallsignCardRecordLoaded;
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
            card.RecordLoaded -= OnCallsignCardRecordLoaded;
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

    private void OnCallsignCardRecordLoaded(object? sender, CallsignRecord record)
    {
        Logger.AcceptLookupRecord(record);
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

        if (_switchableEngine is not null)
        {
            _switchableEngine.Dispose();
        }
        else if (_engine is IDisposable disposable)
        {
            disposable.Dispose();
        }
    }

    /// <summary>
    /// Restores persisted UI preferences (rig control, space weather, inspector).
    /// Call after construction but before or during <see cref="ActivateDashboardAsync"/>.
    /// </summary>
    internal void ApplyPreferences(UiPreferences? prefs)
    {
        if (prefs is null)
        {
            return;
        }

        if (prefs.IsRigEnabled)
        {
            ToggleRigControl();
        }

        if (prefs.IsSpaceWeatherVisible)
        {
            IsSpaceWeatherVisible = true;
        }

        if (prefs.IsInspectorOpen)
        {
            IsInspectorOpen = true;
        }

        _preferredEngineProfileId = string.IsNullOrWhiteSpace(prefs.EngineProfileId)
            ? null
            : prefs.EngineProfileId.Trim();
        _preferredEngineEndpoint = string.IsNullOrWhiteSpace(prefs.EngineEndpoint)
            ? null
            : prefs.EngineEndpoint.Trim();
    }

    /// <summary>
    /// Captures current UI toggle state for persistence across restarts.
    /// </summary>
    internal UiPreferences CapturePreferences() => new()
    {
        IsRigEnabled = IsRigEnabled,
        IsSpaceWeatherVisible = IsSpaceWeatherVisible,
        IsInspectorOpen = IsInspectorOpen,
        EngineProfileId = _switchableEngine?.CurrentProfile.ProfileId,
        EngineEndpoint = _switchableEngine?.CurrentEndpoint,
    };

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
        ActiveLogText = BuildLogText(state.Status);
        ActiveProfileText = BuildProfileText(activeProfile);
        ActiveStationText = BuildStationText(activeProfile);
    }

    private static string BuildLogText(QsoRipper.Services.SetupStatus status)
    {
        var persistenceFields = PersistenceSetupFields.FromStatus(status, status.SuggestedLogFilePath ?? string.Empty);
        var pathValue = PersistenceSetupFields.GetPathValue(persistenceFields);
        if (string.IsNullOrWhiteSpace(pathValue))
        {
            return "Log: engine-managed";
        }

        return $"Log: {Path.GetFileNameWithoutExtension(pathValue.Trim())}";
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
