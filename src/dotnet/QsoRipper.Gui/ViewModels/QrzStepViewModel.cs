using System.Collections.Generic;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Gui.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class QrzStepViewModel : WizardStepViewModel
{
    private readonly IEngineClient _engine;

    public override string Title => "QRZ (optional)";
    public override string Description =>
        "Connect to QRZ for callsign lookups and logbook sync. You can skip this step.";
    public override bool IsSkippable => true;

    [ObservableProperty]
    private string? _username;

    [ObservableProperty]
    private string? _password;

    [ObservableProperty]
    private bool _isTesting;

    [ObservableProperty]
    private string? _testResult;

    [ObservableProperty]
    private bool _testSucceeded;

    public QrzStepViewModel(IEngineClient engine)
    {
        _engine = engine;
    }

    public override Dictionary<string, string> GetFields()
    {
        return new Dictionary<string, string>
        {
            ["qrz_username"] = Username ?? string.Empty,
            ["qrz_password"] = Password ?? string.Empty,
        };
    }

    [RelayCommand]
    private async Task TestConnectionAsync()
    {
        if (string.IsNullOrWhiteSpace(Username) || string.IsNullOrWhiteSpace(Password))
        {
            TestResult = "Username and password are required.";
            TestSucceeded = false;
            return;
        }

        IsTesting = true;
        TestResult = null;
        try
        {
            var result = await _engine.TestQrzCredentialsAsync(Username, Password);
            TestSucceeded = result.Success;
            TestResult = result.Success
                ? "✓ Connected to QRZ successfully!"
                : $"✗ {result.ErrorMessage}";
        }
        catch (Grpc.Core.RpcException ex)
        {
            TestSucceeded = false;
            TestResult = $"✗ Connection failed: {ex.Status.Detail}";
        }
        finally
        {
            IsTesting = false;
        }
    }

    public override void ClearErrors()
    {
        base.ClearErrors();
        TestResult = null;
    }
}
