using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Gui.Services;
using QsoRipper.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class SetupWizardViewModel : ObservableObject
{
    private readonly IEngineClient _engine;
    private readonly MainWindowViewModel _owner;

    public ObservableCollection<WizardStepViewModel> Steps { get; } = [];

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(CurrentStep))]
    [NotifyPropertyChangedFor(nameof(StepLabel))]
    [NotifyPropertyChangedFor(nameof(CanGoBack))]
    [NotifyPropertyChangedFor(nameof(IsLastStep))]
    private int _currentStepIndex;

    [ObservableProperty]
    private bool _isBusy;

    [ObservableProperty]
    private string? _errorMessage;

    public WizardStepViewModel? CurrentStep =>
        CurrentStepIndex >= 0 && CurrentStepIndex < Steps.Count
            ? Steps[CurrentStepIndex]
            : null;

    public string StepLabel => $"Step {CurrentStepIndex + 1} of {Steps.Count}";
    public bool CanGoBack => CurrentStepIndex > 0;
    public bool IsLastStep => CurrentStepIndex == Steps.Count - 1;

    public SetupWizardViewModel(IEngineClient engine, MainWindowViewModel owner)
    {
        _engine = engine;
        _owner = owner;

        Steps.Add(new LogFileStepViewModel());
        Steps.Add(new StationProfileStepViewModel());
        Steps.Add(new QrzStepViewModel(engine));
        Steps.Add(new ReviewStepViewModel());
    }

    /// <summary>
    /// Loads current wizard state from the engine, pre-filling fields.
    /// </summary>
    public async Task LoadStateAsync()
    {
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var state = await _engine.GetWizardStateAsync();

            if (Steps[0] is LogFileStepViewModel logStep)
            {
                logStep.LogFilePath = state.Status.SuggestedLogFilePath;
                foreach (var ss in state.Steps)
                {
                    if (ss.Step == SetupWizardStep.LogFile && ss.Complete)
                    {
                        logStep.LogFilePath = state.Status.LogFilePath;
                    }
                }
            }

            if (Steps[1] is StationProfileStepViewModel stationStep)
            {
                foreach (var ss in state.Steps)
                {
                    if (ss.Step == SetupWizardStep.StationProfiles && ss.Complete)
                    {
                        var active = state.StationProfiles.FirstOrDefault(p => p.IsActive);
                        if (active?.Profile is not null)
                        {
                            stationStep.ProfileName = active.Profile.ProfileName;
                            stationStep.Callsign = active.Profile.StationCallsign;
                            stationStep.OperatorCallsign = active.Profile.OperatorCallsign;
                            stationStep.GridSquare = active.Profile.Grid;
                            stationStep.OperatorName = active.Profile.OperatorName;
                            stationStep.County = active.Profile.County;
                            stationStep.State = active.Profile.State;
                            stationStep.Country = active.Profile.Country;
                            stationStep.ArrlSection = active.Profile.ArrlSection;
                            if (active.Profile.Dxcc != 0)
                            {
                                stationStep.Dxcc = active.Profile.Dxcc.ToString(System.Globalization.CultureInfo.InvariantCulture);
                            }

                            if (active.Profile.CqZone != 0)
                            {
                                stationStep.CqZone = active.Profile.CqZone.ToString(System.Globalization.CultureInfo.InvariantCulture);
                            }

                            if (active.Profile.ItuZone != 0)
                            {
                                stationStep.ItuZone = active.Profile.ItuZone.ToString(System.Globalization.CultureInfo.InvariantCulture);
                            }

                            if (active.Profile.Latitude != 0)
                            {
                                stationStep.Latitude = active.Profile.Latitude.ToString(System.Globalization.CultureInfo.InvariantCulture);
                            }

                            if (active.Profile.Longitude != 0)
                            {
                                stationStep.Longitude = active.Profile.Longitude.ToString(System.Globalization.CultureInfo.InvariantCulture);
                            }
                        }
                    }
                }
            }

            foreach (var ss in state.Steps)
            {
                var idx = StepIndex(ss.Step);
                if (idx >= 0 && idx < Steps.Count)
                {
                    Steps[idx].IsComplete = ss.Complete;
                }
            }
        }
        catch (Grpc.Core.RpcException ex)
        {
            ErrorMessage = $"Failed to load wizard state: {ex.Status.Detail}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private async Task NextAsync()
    {
        if (CurrentStep is null)
        {
            return;
        }

        if (IsLastStep)
        {
            await SaveAsync();
            return;
        }

        // Validate current step before advancing
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var request = BuildValidationRequest(CurrentStepIndex);
            var result = await _engine.ValidateStepAsync(request);

            if (!result.Valid)
            {
                CurrentStep.ApplyValidationErrors(result.Fields);
                ErrorMessage = "Please fix the errors above before continuing.";
                return;
            }

            CurrentStep.IsComplete = true;
            CurrentStep.ClearErrors();

            // Pre-fill review step
            if (CurrentStepIndex + 1 == Steps.Count - 1 && Steps[^1] is ReviewStepViewModel review)
            {
                review.UpdateSummary(Steps.Take(Steps.Count - 1).ToList());
            }

            CurrentStepIndex++;
        }
        catch (Grpc.Core.RpcException ex)
        {
            ErrorMessage = $"Validation failed: {ex.Status.Detail}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    [RelayCommand]
    private void Back()
    {
        if (CanGoBack)
        {
            ErrorMessage = null;
            CurrentStepIndex--;
        }
    }

    [RelayCommand]
    private void Skip()
    {
        // Only QRZ step is skippable
        if (CurrentStep is QrzStepViewModel)
        {
            CurrentStep.IsComplete = true;
            CurrentStep.ClearErrors();

            if (CurrentStepIndex + 1 == Steps.Count - 1 && Steps[^1] is ReviewStepViewModel review)
            {
                review.UpdateSummary(Steps.Take(Steps.Count - 1).ToList());
            }

            CurrentStepIndex++;
        }
    }

    [RelayCommand]
    private void Cancel()
    {
        _owner.CancelWizard();
    }

    [RelayCommand]
    private void NavigateToStep(int stepIndex)
    {
        // Allow navigating to completed steps or the current step
        if (stepIndex >= 0 && stepIndex < Steps.Count
            && (Steps[stepIndex].IsComplete || stepIndex == CurrentStepIndex))
        {
            ErrorMessage = null;
            CurrentStepIndex = stepIndex;
        }
    }

    private async Task SaveAsync()
    {
        IsBusy = true;
        ErrorMessage = null;
        try
        {
            var logStep = Steps.OfType<LogFileStepViewModel>().First();
            var stationStep = Steps.OfType<StationProfileStepViewModel>().First();
            var qrzStep = Steps.OfType<QrzStepViewModel>().First();

            var request = new SaveSetupRequest
            {
                LogFilePath = logStep.LogFilePath ?? string.Empty,
                StationProfile = BuildStationProfile(stationStep),
            };

            if (!string.IsNullOrWhiteSpace(qrzStep.Username))
            {
                request.QrzXmlUsername = qrzStep.Username;
                request.QrzXmlPassword = qrzStep.Password ?? string.Empty;
            }

            var response = await _engine.SaveSetupAsync(request);
            _owner.CloseWizard(setupComplete: response.Status.SetupComplete);
        }
        catch (Grpc.Core.RpcException ex)
        {
            ErrorMessage = $"Save failed: {ex.Status.Detail}";
        }
        finally
        {
            IsBusy = false;
        }
    }

    private ValidateSetupStepRequest BuildValidationRequest(int stepIndex)
    {
        var request = new ValidateSetupStepRequest { Step = StepEnum(stepIndex) };

        switch (Steps[stepIndex])
        {
            case LogFileStepViewModel logStep:
                request.LogFilePath = logStep.LogFilePath ?? string.Empty;
                break;
            case StationProfileStepViewModel stationStep:
                request.StationProfile = BuildStationProfile(stationStep);
                break;
            case QrzStepViewModel qrzStep:
                request.QrzXmlUsername = qrzStep.Username ?? string.Empty;
                request.QrzXmlPassword = qrzStep.Password ?? string.Empty;
                break;
        }

        return request;
    }

    private static int StepIndex(SetupWizardStep step) => step switch
    {
        SetupWizardStep.LogFile => 0,
        SetupWizardStep.StationProfiles => 1,
        SetupWizardStep.QrzIntegration => 2,
        SetupWizardStep.Review => 3,
        _ => -1,
    };

    private static SetupWizardStep StepEnum(int index) => index switch
    {
        0 => SetupWizardStep.LogFile,
        1 => SetupWizardStep.StationProfiles,
        2 => SetupWizardStep.QrzIntegration,
        3 => SetupWizardStep.Review,
        _ => SetupWizardStep.Unspecified,
    };

    private static QsoRipper.Domain.StationProfile BuildStationProfile(StationProfileStepViewModel s)
    {
        var p = new QsoRipper.Domain.StationProfile
        {
            StationCallsign = s.Callsign ?? string.Empty,
            Grid = s.GridSquare ?? string.Empty,
            OperatorName = s.OperatorName ?? string.Empty,
        };

        if (!string.IsNullOrWhiteSpace(s.ProfileName))
        {
            p.ProfileName = s.ProfileName;
        }

        if (!string.IsNullOrWhiteSpace(s.OperatorCallsign))
        {
            p.OperatorCallsign = s.OperatorCallsign;
        }

        if (!string.IsNullOrWhiteSpace(s.County))
        {
            p.County = s.County;
        }

        if (!string.IsNullOrWhiteSpace(s.State))
        {
            p.State = s.State;
        }

        if (!string.IsNullOrWhiteSpace(s.Country))
        {
            p.Country = s.Country;
        }

        if (!string.IsNullOrWhiteSpace(s.ArrlSection))
        {
            p.ArrlSection = s.ArrlSection;
        }

        SetNumericField(s.Dxcc, v => p.Dxcc = v);
        SetNumericField(s.CqZone, v => p.CqZone = v);
        SetNumericField(s.ItuZone, v => p.ItuZone = v);
        SetDoubleField(s.Latitude, v => p.Latitude = v);
        SetDoubleField(s.Longitude, v => p.Longitude = v);

        return p;
    }

    private static void SetNumericField(string? input, Action<uint> setter)
    {
        if (uint.TryParse(input, System.Globalization.CultureInfo.InvariantCulture, out var value))
        {
            setter(value);
        }
    }

    private static void SetDoubleField(string? input, Action<double> setter)
    {
        if (double.TryParse(input, System.Globalization.CultureInfo.InvariantCulture, out var value))
        {
            setter(value);
        }
    }
}
