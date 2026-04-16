using Microsoft.Extensions.Options;
using QsoRipper.DebugHost.Models;
using QsoRipper.DebugHost.Services;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public class DebugWorkbenchStateTests
{
    [Fact]
    public void Initializes_storage_defaults_from_options()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions
        {
            DefaultEngineEndpoint = "http://localhost:60051",
            DefaultEngineStorageBackend = "sqlite",
            DefaultEngineSqlitePath = @".\data\test-qsoripper.db"
        }));

        Assert.Equal("http://localhost:60051", state.EngineEndpoint);
        Assert.Equal("sqlite", state.EngineStorageBackend);
        Assert.Equal(@".\data\test-qsoripper.db", state.EngineSqlitePath);
        Assert.Contains(@"src\rust\target\debug\qsoripper-server", state.BuildEngineLaunchCommand(), StringComparison.Ordinal);
        Assert.Equal("sqlite", state.GetEngineEnvironmentOverrides()["QSORIPPER_STORAGE_BACKEND"]);
        Assert.Equal(@".\data\test-qsoripper.db", state.GetEngineEnvironmentOverrides()["QSORIPPER_SQLITE_PATH"]);
    }

    [Fact]
    public void Update_storage_options_switches_back_to_memory()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateEngineEndpoint(" http://localhost:50061 ");
        state.UpdateStorageOptions("memory", "   ");

        Assert.Equal("http://localhost:50061", state.EngineEndpoint);
        Assert.Equal("memory", state.EngineStorageBackend);
        Assert.Equal("memory", state.GetEngineEnvironmentOverrides()["QSORIPPER_STORAGE_BACKEND"]);
        Assert.DoesNotContain("QSORIPPER_SQLITE_PATH", state.GetEngineEnvironmentOverrides().Keys, StringComparer.Ordinal);
        Assert.Contains(@"src\rust\target\debug\qsoripper-server", state.BuildEngineLaunchCommand(), StringComparison.Ordinal);
    }

    [Fact]
    public void Update_storage_options_preserves_custom_backend_identifier()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateStorageOptions("file-cache", @".\data\portable-store.db");

        Assert.Equal("file-cache", state.EngineStorageBackend);
        Assert.Equal("File Cache", state.GetStorageBackendDisplayName());
        Assert.Equal("file-cache", state.GetEngineEnvironmentOverrides()["QSORIPPER_STORAGE_BACKEND"]);
        Assert.Equal(@".\data\portable-store.db", state.GetEngineEnvironmentOverrides()["QSORIPPER_SQLITE_PATH"]);
    }

    [Fact]
    public void Initializes_dotnet_engine_defaults_from_options()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions
        {
            DefaultEngineProfile = "dotnet",
            DefaultEngineEndpoint = ""
        }));

        Assert.Equal(KnownEngineProfiles.LocalDotNet, state.EngineProfile.ProfileId);
        Assert.Equal(EngineCatalog.DefaultDotNetEndpoint, state.EngineEndpoint);
        Assert.Contains(@"QsoRipper.Engine.DotNet", state.BuildEngineLaunchCommand(), StringComparison.Ordinal);
        Assert.Contains("--listen 127.0.0.1:50052", state.BuildEngineLaunchCommand(), StringComparison.Ordinal);
    }

    [Fact]
    public void Update_engine_profile_swaps_default_endpoint_but_preserves_custom_endpoint()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateEngineProfile(KnownEngineProfiles.LocalDotNet);
        Assert.Equal(EngineCatalog.DefaultDotNetEndpoint, state.EngineEndpoint);

        state.UpdateEngineEndpoint("http://localhost:61234");
        state.UpdateEngineProfile(KnownEngineProfiles.LocalRust);

        Assert.Equal("http://localhost:61234", state.EngineEndpoint);
    }

    [Fact]
    public void Update_runtime_config_syncs_active_storage_and_redacts_secret_values()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateRuntimeConfig(new RuntimeConfigSnapshot
        {
            ActiveStorageBackend = "sqlite",
            PersistenceSummary = "SQLite",
            PersistenceLocation = @".\data\live-qsoripper.db",
            LookupProviderSummary = "QRZ XML capture-only via https://xmldata.qrz.com/xml/current/",
            Values =
            {
                new RuntimeConfigValue
                {
                    Key = "QSORIPPER_STORAGE_BACKEND",
                    HasValue = true,
                    DisplayValue = "sqlite"
                },
                new RuntimeConfigValue
                {
                    Key = "QSORIPPER_SQLITE_PATH",
                    HasValue = true,
                    DisplayValue = @".\data\live-qsoripper.db"
                },
                new RuntimeConfigValue
                {
                    Key = "QSORIPPER_QRZ_XML_PASSWORD",
                    HasValue = true,
                    DisplayValue = "<redacted>",
                    Secret = true,
                    Redacted = true,
                    Overridden = true
                }
            }
        });

        Assert.Equal("sqlite", state.EngineStorageBackend);
        Assert.Equal(@".\data\live-qsoripper.db", state.EngineSqlitePath);
        Assert.Equal("<redacted>", state.GetEngineEnvironmentOverrides()["QSORIPPER_QRZ_XML_PASSWORD"]);
        Assert.Contains(@"src\rust\target\debug\qsoripper-server", state.BuildEngineLaunchCommand(), StringComparison.Ordinal);
    }

    [Fact]
    public void Update_runtime_config_includes_station_profile_environment_overrides()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateRuntimeConfig(new RuntimeConfigSnapshot
        {
            ActiveStorageBackend = "memory",
            ActiveStationProfile = new StationProfile
            {
                StationCallsign = "K7RND",
                Grid = "CN87"
            },
            Values =
            {
                new RuntimeConfigValue
                {
                    Key = "QSORIPPER_STORAGE_BACKEND",
                    HasValue = true,
                    DisplayValue = "memory"
                },
                new RuntimeConfigValue
                {
                    Key = "QSORIPPER_STATION_CALLSIGN",
                    HasValue = true,
                    DisplayValue = "K7RND"
                },
                new RuntimeConfigValue
                {
                    Key = "QSORIPPER_STATION_GRID",
                    HasValue = true,
                    DisplayValue = "CN87"
                }
            }
        });

        Assert.Equal("K7RND", state.GetEngineEnvironmentOverrides()["QSORIPPER_STATION_CALLSIGN"]);
        Assert.Equal("CN87", state.GetEngineEnvironmentOverrides()["QSORIPPER_STATION_GRID"]);
        Assert.Equal("memory", state.EngineStorageBackend);
    }

    [Fact]
    public void Update_setup_status_syncs_persisted_log_file_path()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));

        state.UpdateSetupStatus(new SetupStatus
        {
            ConfigFileExists = true,
            SetupComplete = true,
            ConfigPath = @".\config\config.toml",
#pragma warning disable CS0612 // Type or member is obsolete
            StorageBackend = StorageBackend.Sqlite,
#pragma warning restore CS0612
            LogFilePath = @".\data\portable-qsoripper.db",
            StationProfile = new StationProfile
            {
                StationCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        Assert.NotNull(state.SetupStatus);
        Assert.Null(state.SetupErrorMessage);
        Assert.Equal("memory", state.EngineStorageBackend);
        Assert.Equal(@".\data\portable-qsoripper.db", state.EngineSqlitePath);
    }

    [Fact]
    public void Update_station_profiles_tracks_catalog_and_effective_context()
    {
        var state = new DebugWorkbenchState(Options.Create(new DebugWorkbenchOptions()));
        var catalog = new ListStationProfilesResponse
        {
            ActiveProfileId = "home",
            Profiles =
            {
                new StationProfileRecord
                {
                    ProfileId = "home",
                    IsActive = true,
                    Profile = new StationProfile
                    {
                        ProfileName = "Home",
                        StationCallsign = "K7RND"
                    }
                }
            }
        };
        var context = new ActiveStationContext
        {
            PersistedActiveProfileId = "home",
            PersistedActiveProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND"
            },
            EffectiveActiveProfile = new StationProfile
            {
                ProfileName = "POTA",
                StationCallsign = "K7RND/P"
            },
            HasSessionOverride = true,
            SessionOverrideProfile = new StationProfile
            {
                ProfileName = "POTA",
                StationCallsign = "K7RND/P"
            }
        };

        state.UpdateStationProfiles(catalog, context);

        Assert.NotNull(state.StationProfileCatalog);
        Assert.NotNull(state.ActiveStationContext);
        Assert.Null(state.StationProfileErrorMessage);
        Assert.Equal("home", state.StationProfileCatalog.ActiveProfileId);
        Assert.Equal("K7RND/P", state.ActiveStationContext.EffectiveActiveProfile.StationCallsign);
        Assert.True(state.ActiveStationContext.HasSessionOverride);
    }
}
#pragma warning restore CA1707
