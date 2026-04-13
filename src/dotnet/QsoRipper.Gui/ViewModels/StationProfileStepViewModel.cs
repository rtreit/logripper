using System.Collections.Generic;
using CommunityToolkit.Mvvm.ComponentModel;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class StationProfileStepViewModel : WizardStepViewModel
{
    public override string Title => "Station Info";
    public override string Description => "Tell us about your station.";

    [ObservableProperty]
    private string? _profileName;

    [ObservableProperty]
    private string? _callsign;

    [ObservableProperty]
    private string? _operatorCallsign;

    [ObservableProperty]
    private string? _operatorName;

    [ObservableProperty]
    private string? _gridSquare;

    [ObservableProperty]
    private string? _county;

    [ObservableProperty]
    private string? _state;

    [ObservableProperty]
    private string? _country;

    [ObservableProperty]
    private string? _arrlSection;

    [ObservableProperty]
    private string? _dxcc;

    [ObservableProperty]
    private string? _cqZone;

    [ObservableProperty]
    private string? _ituZone;

    [ObservableProperty]
    private string? _latitude;

    [ObservableProperty]
    private string? _longitude;

    public override Dictionary<string, string> GetFields()
    {
        var fields = new Dictionary<string, string>
        {
            ["profile_name"] = ProfileName ?? string.Empty,
            ["callsign"] = Callsign ?? string.Empty,
            ["operator_callsign"] = OperatorCallsign ?? string.Empty,
            ["grid_square"] = GridSquare ?? string.Empty,
            ["operator_name"] = OperatorName ?? string.Empty,
        };

        AddIfNotEmpty(fields, "county", County);
        AddIfNotEmpty(fields, "state", State);
        AddIfNotEmpty(fields, "country", Country);
        AddIfNotEmpty(fields, "arrl_section", ArrlSection);
        AddIfNotEmpty(fields, "dxcc", Dxcc);
        AddIfNotEmpty(fields, "cq_zone", CqZone);
        AddIfNotEmpty(fields, "itu_zone", ItuZone);
        AddIfNotEmpty(fields, "latitude", Latitude);
        AddIfNotEmpty(fields, "longitude", Longitude);

        return fields;
    }

    private static void AddIfNotEmpty(Dictionary<string, string> dict, string key, string? value)
    {
        if (!string.IsNullOrWhiteSpace(value))
        {
            dict[key] = value;
        }
    }
}
