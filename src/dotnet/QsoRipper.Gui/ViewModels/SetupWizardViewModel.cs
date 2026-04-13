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
    private readonly EngineGrpcService _engine;
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

    public SetupWizardViewModel(EngineGrpcService engine, MainWindowViewModel owner)
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
                            stationStep.Callsign = active.Profile.StationCallsign;
                            stationStep.GridSquare = active.Profile.Grid;
                            stationStep.OperatorName = active.Profile.OperatorName;
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
        _owner.CloseWizard(setupComplete: false);
    }

    [RelayCommand]
    private void NavigateToStep(int stepIndex)
    {
        if (stepIndex >= 0 && stepIndex < Steps.Count && Steps[stepIndex].IsComplete)
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

            var profile = new QsoRipper.Domain.StationProfile
            {
                StationCallsign = stationStep.Callsign ?? string.Empty,
                Grid = stationStep.GridSquare ?? string.Empty,
                OperatorName = stationStep.OperatorName ?? string.Empty,
            };

            if (!string.IsNullOrWhiteSpace(stationStep.County))
            {
                profile.County = stationStep.County;
            }

            if (!string.IsNullOrWhiteSpace(stationStep.State))
            {
                profile.State = stationStep.State;
            }

            if (!string.IsNullOrWhiteSpace(stationStep.Country))
            {
                profile.Country = stationStep.Country;
            }

            if (!string.IsNullOrWhiteSpace(stationStep.ArrlSection))
            {
                profile.ArrlSection = stationStep.ArrlSection;
            }

            var request = new SaveSetupRequest
            {
                LogFilePath = logStep.LogFilePath ?? string.Empty,
                StationProfile = profile,
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
                var profile = new QsoRipper.Domain.StationProfile
                {
                    StationCallsign = stationStep.Callsign ?? string.Empty,
                    Grid = stationStep.GridSquare ?? string.Empty,
                    OperatorName = stationStep.OperatorName ?? string.Empty,
                };
                if (!string.IsNullOrWhiteSpace(stationStep.County))
                {
                    profile.County = stationStep.County;
                }

                if (!string.IsNullOrWhiteSpace(stationStep.State))
                {
                    profile.State = stationStep.State;
                }

                if (!string.IsNullOrWhiteSpace(stationStep.Country))
                {
                    profile.Country = stationStep.Country;
                }

                if (!string.IsNullOrWhiteSpace(stationStep.ArrlSection))
                {
                    profile.ArrlSection = stationStep.ArrlSection;
                }

                request.StationProfile = profile;
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
}
