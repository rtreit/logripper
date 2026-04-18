using System.ComponentModel.DataAnnotations;
using System.Linq;
using QsoRipper.DebugHost.Models;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Tests;

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
                RigControl = new RigControlSettings
                {
                    Enabled = true,
                    Host = "127.0.0.1",
                    Port = 4532,
                    ReadTimeoutMs = 2000,
                    StaleThresholdMs = 5000
                },
                StationProfile = new StationProfile
                {
                    ProfileName = "Home",
                    StationCallsign = "K7RND",
                    Grid = "CN87"
                }
            },
            @".\data\fallback.db");

        Assert.Equal(@".\data\portable.db", model.LogFilePath);
        Assert.True(model.RequiresLogFilePath);
        Assert.Equal("k7rnd", model.QrzXmlUsername);
        Assert.True(model.RigControlEnabled);
        Assert.Equal("127.0.0.1", model.RigControlHost);
        Assert.Equal(4532, model.RigControlPort);
        Assert.Equal(2000, model.RigControlReadTimeoutMs);
        Assert.Equal(5000, model.RigControlStaleThresholdMs);
        Assert.Equal("Home", model.ProfileName);
        Assert.Equal("K7RND", model.StationCallsign);
        Assert.Equal("CN87", model.Grid);
    }

    [Fact]
    public void SetupEditorModel_to_request_trims_and_omits_blank_optionals()
    {
        var model = new SetupEditorModel
        {
            RequiresLogFilePath = true,
            LogFilePath = @"  .\data\qsoripper.db  ",
            QrzXmlUsername = "  k7rnd  ",
            QrzXmlPassword = "  secret  ",
            RigControlEnabled = true,
            RigControlHost = " 127.0.0.1 ",
            RigControlPort = 4532,
            RigControlReadTimeoutMs = 2000,
            RigControlStaleThresholdMs = 5000,
            ProfileName = "  Home  ",
            StationCallsign = "  K7RND  ",
            OperatorCallsign = "  K7RND  ",
            Grid = "  CN87  ",
            Latitude = 47.6
        };

        var request = model.ToRequest();

        Assert.True(request.HasLogFilePath);
        Assert.Equal(@".\data\qsoripper.db", request.LogFilePath);
        var persistenceValue = Assert.Single(request.PersistenceValues);
        Assert.Equal(PersistenceSetup.PathKey, persistenceValue.Key);
        Assert.Equal(@".\data\qsoripper.db", persistenceValue.Value);
        Assert.True(request.HasQrzXmlUsername);
        Assert.True(request.HasQrzXmlPassword);
        Assert.Equal("k7rnd", request.QrzXmlUsername);
        Assert.Equal("secret", request.QrzXmlPassword);
        Assert.NotNull(request.RigControl);
        Assert.True(request.RigControl.Enabled);
        Assert.True(request.RigControl.HasHost);
        Assert.Equal("127.0.0.1", request.RigControl.Host);
        Assert.True(request.RigControl.HasPort);
        Assert.Equal(4532u, request.RigControl.Port);
        Assert.True(request.RigControl.HasReadTimeoutMs);
        Assert.Equal(2000ul, request.RigControl.ReadTimeoutMs);
        Assert.True(request.RigControl.HasStaleThresholdMs);
        Assert.Equal(5000ul, request.RigControl.StaleThresholdMs);
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
            RequiresLogFilePath = true,
            LogFilePath = "   ",
            QrzXmlUsername = "k7rnd",
            StationCallsign = "K7RND"
        };

        var results = Validate(model);

        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.LogFilePath), StringComparer.Ordinal));
        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.QrzXmlPassword), StringComparer.Ordinal));
    }

    [Fact]
    public void SetupEditorModel_validate_skips_log_file_when_persistence_is_not_required()
    {
        var model = new SetupEditorModel
        {
            RequiresLogFilePath = false,
            LogFilePath = "   ",
            StationCallsign = "K7RND"
        };

        var results = Validate(model);

        Assert.DoesNotContain(
            results,
            result => result.MemberNames.Contains(nameof(SetupEditorModel.LogFilePath), StringComparer.Ordinal));
    }

    [Fact]
    public void SetupEditorModel_create_skips_legacy_path_when_persistence_contract_explicitly_omits_inputs()
    {
        var model = SetupEditorModel.Create(
            new SetupStatus
            {
                PersistenceContractExplicit = true,
                PersistenceStepEnabled = false,
                PersistenceLabel = "Storage",
                PersistenceDescription = "No setup input required for this engine."
            },
            @".\data\fallback.db");

        Assert.False(model.HasPersistenceInputs);
        Assert.False(model.RequiresLogFilePath);
        Assert.Equal(string.Empty, model.LogFilePath);
    }

    [Fact]
    public void SetupEditorModel_to_request_preserves_multiple_persistence_fields()
    {
        var model = SetupEditorModel.Create(
            new SetupStatus
            {
                PersistenceLabel = "Storage",
                PersistenceDescription = "Configure persistent storage for the selected engine.",
                PersistenceStepEnabled = true,
                SuggestedLogFilePath = @".\data\portable.db",
                PersistenceDefinitions =
                {
                    new RuntimeConfigDefinition
                    {
                        Key = PersistenceSetup.PathKey,
                        Label = "Path",
                        Kind = RuntimeConfigValueKind.Path,
                        Required = true,
                    },
                    new RuntimeConfigDefinition
                    {
                        Key = "persistence.bucket",
                        Label = "Bucket",
                        Kind = RuntimeConfigValueKind.String,
                        Required = true,
                    }
                },
                PersistenceValues =
                {
                    new RuntimeConfigValue
                    {
                        Key = PersistenceSetup.PathKey,
                        DisplayValue = @".\data\portable.db",
                    },
                    new RuntimeConfigValue
                    {
                        Key = "persistence.bucket",
                        HasValue = true,
                        DisplayValue = "portable"
                    }
                }
            },
            @".\data\fallback.db");

        model.LogFilePath = @"  .\data\updated.db  ";
        model.PersistenceFields.Single(field => field.Key == "persistence.bucket").Value = "  archive  ";
        var request = model.ToRequest();

        Assert.False(request.HasLogFilePath);
        Assert.Equal(string.Empty, request.LogFilePath);
        Assert.Equal(2, request.PersistenceValues.Count);
        Assert.Contains(
            request.PersistenceValues,
            value => value.Key == PersistenceSetup.PathKey && value.Value == @".\data\updated.db");
        Assert.Contains(
            request.PersistenceValues,
            value => value.Key == "persistence.bucket" && value.Value == "archive");
    }

    [Fact]
    public void SetupEditorModel_validate_rejects_invalid_rig_control_values()
    {
        var model = new SetupEditorModel
        {
            LogFilePath = @".\data\qsoripper.db",
            StationCallsign = "K7RND",
            RigControlPort = 70000,
            RigControlReadTimeoutMs = 0,
            RigControlStaleThresholdMs = 0
        };

        var results = Validate(model);

        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.RigControlPort), StringComparer.Ordinal));
        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.RigControlReadTimeoutMs), StringComparer.Ordinal));
        Assert.Contains(results, result => result.MemberNames.Contains(nameof(SetupEditorModel.RigControlStaleThresholdMs), StringComparer.Ordinal));
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

    [Fact]
    public void StationProfileEditorModel_roundtrips_arrl_section()
    {
        var original = new StationProfile
        {
            StationCallsign = "K7RND",
            ArrlSection = "WWA"
        };

        var model = new StationProfileEditorModel();
        model.LoadFrom("test-id", original);

        Assert.Equal("WWA", model.ArrlSection);

        var roundTripped = model.ToStationProfile();

        Assert.True(roundTripped.HasArrlSection);
        Assert.Equal("WWA", roundTripped.ArrlSection);
    }

    [Fact]
    public void StationProfileEditorModel_clears_arrl_section_when_blank()
    {
        var model = new StationProfileEditorModel
        {
            StationCallsign = "K7RND",
            ArrlSection = "   "
        };

        var roundTripped = model.ToStationProfile();

        Assert.False(roundTripped.HasArrlSection);
    }

    [Fact]
    public void AdifExportEditorModel_to_request_trims_values_and_parses_utc_filters()
    {
        var model = new AdifExportEditorModel
        {
            AfterUtc = " 2026-04-10T00:00:00Z ",
            BeforeUtc = "2026-04-11T12:30:00Z",
            ContestId = "  ARRL-DX-SSB  ",
            IncludeHeader = true
        };

        var request = model.ToRequest();

        Assert.True(request.IncludeHeader);
        Assert.NotNull(request.After);
        Assert.NotNull(request.Before);
        Assert.Equal("ARRL-DX-SSB", request.ContestId);
    }

    [Fact]
    public void AdifExportEditorModel_validate_rejects_invalid_and_reversed_ranges()
    {
        var invalid = new AdifExportEditorModel
        {
            AfterUtc = "not-a-time"
        };

        var invalidResults = Validate(invalid);

        Assert.Contains(invalidResults, result => result.MemberNames.Contains(nameof(AdifExportEditorModel.AfterUtc), StringComparer.Ordinal));

        var reversed = new AdifExportEditorModel
        {
            AfterUtc = "2026-04-12T00:00:00Z",
            BeforeUtc = "2026-04-11T00:00:00Z"
        };

        var reversedResults = Validate(reversed);

        Assert.Contains(reversedResults, result => result.MemberNames.Contains(nameof(AdifExportEditorModel.BeforeUtc), StringComparer.Ordinal));
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
