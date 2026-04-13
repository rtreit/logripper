using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls.ApplicationLifetimes;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class MainWindowViewModel : ObservableObject
{
    private readonly EngineGrpcService _engine;

    [ObservableProperty]
    private bool _isWizardOpen;

    [ObservableProperty]
    private SetupWizardViewModel? _wizardViewModel;

    [ObservableProperty]
    private string _statusMessage = "Checking engine connection…";

    [ObservableProperty]
    private bool _isSetupIncomplete;

    public MainWindowViewModel(EngineGrpcService engine)
    {
        _engine = engine;
    }

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
                await OpenWizardAsync();
            }
            else
            {
                StatusMessage = "Ready — setup complete.";
                IsSetupIncomplete = false;
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
        var vm = new SetupWizardViewModel(_engine, this);
        WizardViewModel = vm;
        IsWizardOpen = true;
        await vm.LoadStateAsync();
    }

    [RelayCommand]
    private static void Exit()
    {
        if (Application.Current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime lifetime)
        {
            lifetime.Shutdown();
        }
    }

    internal void CloseWizard(bool setupComplete)
    {
        IsWizardOpen = false;
        WizardViewModel = null;
        IsSetupIncomplete = !setupComplete;
        StatusMessage = setupComplete
            ? "Ready — setup complete."
            : "Setup incomplete — open Settings to finish.";
    }
}
