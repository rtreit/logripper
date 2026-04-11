using System.ComponentModel.DataAnnotations;
using LogRipper.DebugHost.Models;
using LogRipper.Domain;
using LogRipper.Services;

namespace LogRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class EditorModelTests
{
    [Fact]
    public void SetupEditorModel_create_uses_persisted_status_and_defaults()
    {
        var model = SetupEditorModel.Create(
            new SetupStatus
            {
                LogFilePath = @".\data\portable.db",
                QrzXmlUsername = "k7rnd",
                StationProfile = new StationProfile
                {
                    ProfileName = "Home",
                    StationCallsign = "K7RND",
                    Grid = "CN87"
                }
            },
            @".\data\fallback.db");

        Assert.Equal(@".\data\portable.db", model.LogFilePath);
        Assert.Equal("k7rnd", model.QrzXmlUsername);
        Assert.Equal("Home", model.ProfileName);
        Assert.Equal("K7RND", model.StationCallsign);
        Assert.Equal("CN87", model.Grid);
    }

    [Fact]
    public void SetupEditorModel_to_request_trims_and_omits_blank_optionals()
    {
        var model = new SetupEditorModel
        {
            LogFilePath = @"  .\data\logripper.db  ",
            QrzXmlUsername = "  k7rnd  ",
            QrzXmlPassword = "  secret  ",
            ProfileName = "  Home  ",
            StationCallsign = "  K7RND  ",
            OperatorCallsign = "  K7RND  ",
            Grid = "  CN87  ",
            Latitude = 47.6
        };

        var request = model.ToRequest();

        Assert.True(request.HasLogFilePath);
        Assert.Equal(@".\data\logripper.db", request.LogFilePath);
        Assert.True(request.HasQrzXmlUsername);
        Assert.True(request.HasQrzXmlPassword);
        Assert.Equal("k7rnd", request.QrzXmlUsername);
        Assert.Equal("secret", request.QrzXmlPassword);
        Assert.NotNull(request.StationProfile);
        Assert.Equal("Home", request.StationProfile.ProfileName);
        Assert.Equal("K7RND", request.StationProfile.StationCallsign);
        Assert.Equal("K7RND", request.StationProfile.OperatorCallsign);
        Assert.Equal("CN87", request.StationProfile.Grid);
        Assert.True(request.StationProfile.HasLatitude);
    }

    [Fact]
    public void SetupEditorModel_validate_requires_log_file_path_and_complete_qrz_pair()
    {
        var model = new SetupEditorModel
        {
            LogFilePath = "   ",
            QrzXmlUsername = "k7rnd",
            StationCallsign = "K7RND"
        };

        var results = Validate(model);

        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.LogFilePath), StringComparer.Ordinal));
        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.QrzXmlPassword), StringComparer.Ordinal));
    }

    [Fact]
    public void StationProfileEditorModel_to_request_omits_blank_profile_id_and_optional_fields()
    {
        var model = new StationProfileEditorModel
        {
            ProfileId = "   ",
            MakeActive = true,
            StationCallsign = " K7RND ",
            Country = " United States ",
            Dxcc = 291
        };

        var request = model.ToRequest();

        Assert.False(request.HasProfileId);
        Assert.True(request.MakeActive);
        Assert.NotNull(request.Profile);
        Assert.Equal("K7RND", request.Profile.StationCallsign);
        Assert.Equal("United States", request.Profile.Country);
        Assert.True(request.Profile.HasDxcc);
        Assert.Equal((uint)291, request.Profile.Dxcc);
        Assert.False(request.Profile.HasGrid);
    }

    [Fact]
    public void StationProfileEditorModel_numeric_fields_use_blazor_supported_editor_types()
    {
        Assert.Equal(typeof(int?), typeof(StationProfileEditorModel).GetProperty(nameof(StationProfileEditorModel.Dxcc))?.PropertyType);
        Assert.Equal(typeof(int?), typeof(StationProfileEditorModel).GetProperty(nameof(StationProfileEditorModel.CqZone))?.PropertyType);
        Assert.Equal(typeof(int?), typeof(StationProfileEditorModel).GetProperty(nameof(StationProfileEditorModel.ItuZone))?.PropertyType);
    }

    [Fact]
    public void StationProfileEditorModel_validate_rejects_non_positive_zone_values()
    {
        var model = new StationProfileEditorModel
        {
            StationCallsign = "K7RND",
            Dxcc = 0,
            CqZone = -1,
            ItuZone = 0
        };

        var results = Validate(model);

        Assert.Contains(results, result => result.MemberNames.Contains(nameof(StationProfileEditorModel.Dxcc), StringComparer.Ordinal));
        Assert.Contains(results, result => result.MemberNames.Contains(nameof(StationProfileEditorModel.CqZone), StringComparer.Ordinal));
        Assert.Contains(results, result => result.MemberNames.Contains(nameof(StationProfileEditorModel.ItuZone), StringComparer.Ordinal));
    }

    private static List<ValidationResult> Validate(object model)
    {
        var results = new List<ValidationResult>();
        Validator.TryValidateObject(
            model,
            new ValidationContext(model),
            results,
            validateAllProperties: true);
        return results;
    }
}
#pragma warning restore CA1707
