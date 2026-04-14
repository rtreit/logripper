using System.Globalization;
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
    private bool _isWizardOpen;

    [ObservableProperty]
    private SetupWizardViewModel? _wizardViewModel;

    [ObservableProperty]
    private string _statusMessage = "Checking engine connection...";

    [ObservableProperty]
    private bool _isSetupIncomplete;

    [ObservableProperty]
    private string _currentUtcTime = string.Empty;

    [ObservableProperty]
    private string _currentUtcDate = string.Empty;

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
    /// Called after the main window has loaded. Checks first-run state.
    /// </summary>
    public async Task CheckFirstRunAsync()
    {
        try
        {
            var state = await _engine.GetWizardStateAsync();
            if (state.Status.IsFirstRun || !state.Status.SetupComplete)
            {
                IsSetupIncomplete = !state.Status.SetupComplete;
                StatusMessage = IsSetupIncomplete
                    ? "Setup incomplete - finish settings to start logging."
                    : "Welcome to QsoRipper.";
                await OpenWizardAsync();
            }
            else
            {
                IsSetupIncomplete = false;
                await ActivateDashboardAsync(focusSearch: true);
            }
        }
        catch (Grpc.Core.RpcException)
        {
            StatusMessage = "Cannot connect to engine at 127.0.0.1:50051. Is the engine running?";
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
    private void FocusSearch()
    {
        if (!IsWizardOpen)
        {
            SearchFocusRequested?.Invoke(this, EventArgs.Empty);
        }
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
            ? "Ready - setup complete."
            : "Setup incomplete - open Settings to finish.";

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
            ? "Ready - setup complete."
            : "Setup incomplete - open Settings to finish.";

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
        StatusMessage = "Ready - setup complete.";
        await RecentQsos.RefreshAsync();

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
}
