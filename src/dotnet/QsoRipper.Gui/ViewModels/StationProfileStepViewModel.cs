using System.Collections.Generic;
using CommunityToolkit.Mvvm.ComponentModel;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class StationProfileStepViewModel : WizardStepViewModel
{
    public override string Title => "Station Info";
    public override string Description => "Tell us about your station.";

    [ObservableProperty]
    private string? _callsign;

    [ObservableProperty]
    private string? _gridSquare;

    [ObservableProperty]
    private string? _operatorName;

    [ObservableProperty]
    private string? _county;

    [ObservableProperty]
    private string? _state;

    [ObservableProperty]
    private string? _country;

    [ObservableProperty]
    private string? _arrlSection;

    public override Dictionary<string, string> GetFields()
    {
        var fields = new Dictionary<string, string>
        {
            ["callsign"] = Callsign ?? string.Empty,
            ["grid_square"] = GridSquare ?? string.Empty,
            ["operator_name"] = OperatorName ?? string.Empty,
        };

        if (!string.IsNullOrWhiteSpace(County))
        {
            fields["county"] = County;
        }

        if (!string.IsNullOrWhiteSpace(State))
        {
            fields["state"] = State;
        }

        if (!string.IsNullOrWhiteSpace(Country))
        {
            fields["country"] = Country;
        }

        if (!string.IsNullOrWhiteSpace(ArrlSection))
        {
            fields["arrl_section"] = ArrlSection;
        }

        return fields;
    }
}
