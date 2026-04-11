using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Models;

internal sealed class StationProfileEditorModel : StationProfileEditorModelBase
{
    public string? ProfileId { get; set; }

    public bool MakeActive { get; set; }

    public static StationProfileEditorModel CreateEmpty()
    {
        return new StationProfileEditorModel();
    }

    public void LoadFrom(string? profileId, StationProfile? profile, bool makeActive = false)
    {
        ProfileId = NormalizeOptional(profileId);
        MakeActive = makeActive;
        base.LoadFrom(profile);
    }

    public SaveStationProfileRequest ToRequest()
    {
        var request = new SaveStationProfileRequest
        {
            Profile = ToStationProfile(),
            MakeActive = MakeActive
        };

        if (!string.IsNullOrWhiteSpace(ProfileId))
        {
            request.ProfileId = ProfileId.Trim();
        }

        return request;
    }

    public void Reset()
    {
        ProfileId = null;
        MakeActive = false;
        ResetStationFields();
    }
}
