using System.Net.Http;
using System.Reflection;
using System.Text;
using Google.Protobuf.WellKnownTypes;
using Grpc.Core;
using QsoRipper.Domain;
using QsoRipper.Engine.DotNet;
using QsoRipper.Engine.QrzLogbook;
using QsoRipper.Engine.RigControl;
using QsoRipper.Engine.Storage.Memory;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Engine.DotNet.Tests;

#pragma warning disable CA1707 // Remove underscores from member names - xUnit allows underscores in test methods
public sealed class ManagedEngineStateTests : IDisposable
{
    private readonly string _tempDirectory;

    public ManagedEngineStateTests()
    {
        _tempDirectory = Path.Combine(
            Path.GetTempPath(),
            "qsoripper-managed-engine-tests",
            Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(_tempDirectory);
    }

    [Fact]
    public void Build_engine_info_reports_managed_engine_identity()
    {
        var info = ManagedEngineState.BuildEngineInfo();

        Assert.Equal(EngineCatalog.DotNetProfile.EngineId, info.EngineId);
        Assert.Equal(EngineCatalog.DotNetProfile.DisplayName, info.DisplayName);
        Assert.Contains("engine-info", info.Capabilities);
        Assert.Contains("logbook", info.Capabilities);
        Assert.Contains("lookup-callsign", info.Capabilities);
        Assert.Contains("lookup-stream", info.Capabilities);
        Assert.Contains("lookup-cache", info.Capabilities);
        Assert.Contains("rig-control", info.Capabilities);
    }

    [Fact]
    public void Save_setup_ignores_persistence_paths_and_redacts_runtime_values()
    {
        var state = CreateState();

        var response = state.SaveSetup(new SaveSetupRequest
        {
            LogFilePath = Path.Combine(_tempDirectory, "portable-log.db"),
            PersistenceValues =
            {
                new SetupFieldValue
                {
                    Key = "persistence.path",
                    Value = Path.Combine(_tempDirectory, "portable-from-contract.db")
                }
            },
            QrzXmlUsername = "k7rnd",
            QrzXmlPassword = "secret",
            QrzLogbookApiKey = "api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var runtime = state.GetRuntimeConfigSnapshot();
        var profiles = state.ListStationProfiles();
        var persistedConfig = File.ReadAllText(Path.Combine(_tempDirectory, "config.toml"));

        Assert.True(response.Status.SetupComplete);
        Assert.True(response.Status.ConfigFileExists);
        Assert.True(response.Status.PersistenceContractExplicit);
        Assert.True(string.IsNullOrWhiteSpace(response.Status.LogFilePath));
        Assert.True(string.IsNullOrWhiteSpace(response.Status.SuggestedLogFilePath));
        Assert.Equal("K7RND", response.Status.StationProfile.StationCallsign);
        Assert.Single(profiles.Profiles);
        Assert.NotEmpty(profiles.ActiveProfileId);
        Assert.Contains(response.Status.Warnings, warning => warning.Contains("in-memory logbook", StringComparison.Ordinal));

        var storageValue = runtime.Values.Single(value => value.Key == "QSORIPPER_STORAGE_BACKEND");
        var passwordValue = runtime.Values.Single(value => value.Key == "QSORIPPER_QRZ_XML_PASSWORD");
        Assert.Equal("memory", storageValue.DisplayValue);
        Assert.Equal("In-memory logbook", runtime.PersistenceSummary);
        Assert.True(string.IsNullOrWhiteSpace(runtime.PersistenceLocation));
        Assert.DoesNotContain(runtime.Values, value => value.Key == "QSORIPPER_SQLITE_PATH");
        Assert.Equal("***", passwordValue.DisplayValue);
        Assert.True(passwordValue.Secret);
        Assert.True(passwordValue.Redacted);
        Assert.DoesNotContain("log_file_path", persistedConfig, StringComparison.OrdinalIgnoreCase);
    }

    [Fact]
    public void Log_qso_uses_active_station_context_and_sync_updates_status()
    {
        var state = CreateStateWithSync();
        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var logged = state.LogQso(new LogQsoRequest
        {
            SyncToQrz = false,
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-04-12T01:51:00Z", System.Globalization.CultureInfo.InvariantCulture))
            }
        });

        var stored = state.GetQso(logged.LocalId);
        var beforeSync = state.GetSyncStatus();
        var syncResult = state.SyncWithQrz();
        var afterSync = state.GetSyncStatus();

        Assert.NotNull(stored);
        Assert.Equal("K7RND", stored!.StationCallsign);
        Assert.Equal("K7RND", stored.StationSnapshot.StationCallsign);
        Assert.Equal("CN87", stored.StationSnapshot.Grid);
        Assert.Equal(1u, beforeSync.PendingUpload);
        Assert.True(string.IsNullOrEmpty(syncResult.Error), $"Sync error: [{syncResult.Error}]");
        Assert.Equal(1u, syncResult.UploadedRecords);
        Assert.True(syncResult.Complete);
        Assert.Equal(0u, afterSync.PendingUpload);
        // STATUS is fetched once before upload (issue #337 fix), so the count
        // it reports is the pre-upload count (0). The next sync cycle will
        // see the post-upload count.
        Assert.Equal(0u, afterSync.QrzQsoCount);
        Assert.Equal("K7RND", afterSync.QrzLogbookOwner);
    }

    [Fact]
    public void Save_setup_normalizes_unspecified_conflict_policy_to_flag_for_review()
    {
        var state = CreateState();

        var response = state.SaveSetup(new SaveSetupRequest
        {
            SyncConfig = new SyncConfig
            {
                AutoSyncEnabled = true,
                SyncIntervalSeconds = 300,
                ConflictPolicy = ConflictPolicy.Unspecified
            }
        });

        Assert.Equal(ConflictPolicy.FlagForReview, response.Status.SyncConfig.ConflictPolicy);
    }

    [Fact]
    public void Sync_with_qrz_unexpected_exception_does_not_include_stack_trace()
    {
        var storage = new MemoryStorage();
        var syncEngine = new QrzSyncEngine(new FakeMalformedQrzLogbookApi());
        var state = new ManagedEngineState(
            Path.Combine(_tempDirectory, "config.toml"),
            storage,
            lookupCoordinator: null,
            rigControlMonitor: null,
            spaceWeatherMonitor: null,
            syncEngine: syncEngine);

        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var response = state.SyncWithQrz();

        Assert.True(response.Complete);
        Assert.False(string.IsNullOrWhiteSpace(response.Error));
        Assert.DoesNotContain("\n", response.Error, StringComparison.Ordinal);
        Assert.DoesNotContain(" at ", response.Error, StringComparison.Ordinal);
    }

    [Fact]
    public void Apply_runtime_config_rejects_non_memory_storage()
    {
        var state = CreateState();

        var exception = Assert.Throws<InvalidOperationException>(() => state.ApplyRuntimeConfig(
        [
            new RuntimeConfigMutation
            {
                Key = "QSORIPPER_STORAGE_BACKEND",
                Kind = RuntimeConfigMutationKind.Set,
                Value = "sqlite"
            }
        ]));

        Assert.Equal("The managed .NET engine storage backend is fixed at startup. Restart the engine to use 'sqlite'.", exception.Message);
    }

    [Fact]
    public void Runtime_overrides_and_session_override_do_not_persist_across_restart()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            QrzXmlUsername = "k7rnd",
            QrzXmlPassword = "secret",
            QrzLogbookApiKey = "api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        state.ApplyRuntimeConfig(
        [
            new RuntimeConfigMutation
            {
                Key = "QSORIPPER_QRZ_XML_USERNAME",
                Kind = RuntimeConfigMutationKind.Set,
                Value = "runtime-user"
            },
            new RuntimeConfigMutation
            {
                Key = "QSORIPPER_QRZ_XML_PASSWORD",
                Kind = RuntimeConfigMutationKind.Set,
                Value = "runtime-secret"
            },
            new RuntimeConfigMutation
            {
                Key = "QSORIPPER_QRZ_LOGBOOK_API_KEY",
                Kind = RuntimeConfigMutationKind.Clear
            }
        ]);

        state.SetSessionStationProfileOverride(new StationProfile
        {
            ProfileName = "Field Day",
            StationCallsign = "W7FD",
            OperatorCallsign = "W7FD",
            Grid = "CN85"
        });

        var restarted = CreateState();
        var status = restarted.GetSetupStatus();
        var runtime = restarted.GetRuntimeConfigSnapshot();
        var context = restarted.GetActiveStationContext();

        Assert.Equal("k7rnd", status.QrzXmlUsername);
        Assert.True(status.HasQrzXmlPassword);
        Assert.True(status.HasQrzLogbookApiKey);
        Assert.False(context.HasSessionOverride);
        Assert.Equal("K7RND", context.EffectiveActiveProfile.StationCallsign);
        Assert.Equal(
            "k7rnd",
            runtime.Values.Single(value => value.Key == "QSORIPPER_QRZ_XML_USERNAME").DisplayValue);
        Assert.True(
            runtime.Values.Single(value => value.Key == "QSORIPPER_QRZ_LOGBOOK_API_KEY").HasValue);
    }

    [Fact]
    public void Save_setup_rebuilds_owned_sync_client_without_leaking_previous_http_client()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "api-key"
        });

        var originalSyncEngine = GetRequiredPrivateField<QrzSyncEngine>(state, "_syncEngine");
        var originalHttpClient = GetOwnedSyncHttpClient(originalSyncEngine);
        Assert.False(IsHttpClientDisposed(originalHttpClient));

        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "replacement-key"
        });

        Assert.True(IsHttpClientDisposed(originalHttpClient));
    }

    [Fact]
    public void Migrates_legacy_json_config_to_shared_toml()
    {
        var legacyPath = Path.Combine(_tempDirectory, "dotnet-engine.json");
        var configPath = Path.Combine(_tempDirectory, "config.toml");
        File.WriteAllText(
            legacyPath,
            """
            {
              "qrzXmlUsername": "k7rnd",
              "qrzXmlPassword": "secret",
              "hasQrzXmlPassword": true,
              "activeProfileId": "home",
              "stationProfiles": [
                {
                  "profileId": "home",
                  "profileJson": "{ \"profileName\": \"Home\", \"stationCallsign\": \"K7RND\", \"operatorCallsign\": \"K7RND\", \"grid\": \"CN87\" }"
                }
              ],
              "runtimeOverrides": {
                "QSORIPPER_QRZ_XML_USERNAME": "runtime-user"
              },
              "sessionOverrideProfileJson": "{ \"profileName\": \"Field\", \"stationCallsign\": \"W7FD\", \"operatorCallsign\": \"W7FD\", \"grid\": \"CN85\" }"
            }
            """);

        var state = CreateState();
        var status = state.GetSetupStatus();
        var context = state.GetActiveStationContext();
        var persistedConfig = File.ReadAllText(configPath);

        Assert.True(File.Exists(configPath));
        Assert.Contains("active_profile_id = \"home\"", persistedConfig, StringComparison.Ordinal);
        Assert.Contains("station_callsign = \"K7RND\"", persistedConfig, StringComparison.Ordinal);
        Assert.DoesNotContain("runtimeOverrides", persistedConfig, StringComparison.OrdinalIgnoreCase);
        Assert.DoesNotContain("sessionOverrideProfileJson", persistedConfig, StringComparison.OrdinalIgnoreCase);
        Assert.Equal("k7rnd", status.QrzXmlUsername);
        Assert.True(status.HasQrzXmlPassword);
        Assert.Equal("K7RND", status.StationProfile.StationCallsign);
        Assert.False(context.HasSessionOverride);
    }

    [Fact]
    public async Task Delete_qso_grpc_success_omits_optional_error_fields()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var logged = state.LogQso(new LogQsoRequest
        {
            SyncToQrz = false,
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-04-16T22:48:00Z", System.Globalization.CultureInfo.InvariantCulture))
            }
        });

        var service = new ManagedLogbookGrpcService(state);
        var response = await service.DeleteQso(
            new DeleteQsoRequest
            {
                LocalId = logged.LocalId,
                DeleteFromQrz = false
            },
            null!);

        Assert.True(response.Success);
        Assert.True(string.IsNullOrEmpty(response.Error));
        Assert.True(string.IsNullOrEmpty(response.QrzDeleteError));
    }

    [Fact]
    public void Test_rig_connection_connected_omits_error_message()
    {
        var state = CreateStateWithRigSnapshot(new RigSnapshot
        {
            FrequencyHz = 14_074_000,
            Band = Band._20M,
            Mode = Mode.Ft8
        });

        var response = state.TestRigConnection();

        Assert.True(response.Success);
        Assert.True(string.IsNullOrEmpty(response.ErrorMessage));
        Assert.NotNull(response.Snapshot);
        Assert.Equal(14_074_000UL, response.Snapshot.FrequencyHz);
        Assert.Equal(RigConnectionStatus.Connected, response.Snapshot.Status);
    }

    [Fact]
    public void Build_rig_snapshot_connected_omits_error_message_without_monitor()
    {
        var state = CreateStateWithRigSnapshot(new RigSnapshot
        {
            Status = RigConnectionStatus.Connected,
            FrequencyHz = 14_074_000,
            Band = Band._20M,
            Mode = Mode.Ft8,
            SampledAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
        });

        var snapshot = state.BuildRigSnapshot();

        Assert.Equal(RigConnectionStatus.Connected, snapshot.Status);
        Assert.False(snapshot.HasErrorMessage);
        Assert.Equal(14_074_000UL, snapshot.FrequencyHz);
    }

    [Fact]
    public void Log_qso_requires_timestamp_band_and_mode()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var exception = Assert.Throws<InvalidOperationException>(() => state.LogQso(new LogQsoRequest
        {
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW"
            }
        }));

        Assert.Equal("utc_timestamp is required.", exception.Message);
    }

    [Fact]
    public void Import_adif_applies_active_profile_and_skips_duplicates()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var payload = Utf8("<CALL:4>W1AW\n<QSO_DATE:8>20260115\n<TIME_ON:4>1523\n<BAND:3>20M\n<MODE:4>RTTY\n<EOR>\n");

        var first = state.ImportAdif(payload, refresh: false);
        var second = state.ImportAdif(payload, refresh: false);
        var stored = state.ListQsos(new ListQsosRequest()).Single();

        Assert.Equal(1u, first.RecordsImported);
        Assert.Contains(first.Warnings, warning => warning.Contains("applied active station profile 'Home'.", StringComparison.Ordinal));
        Assert.Equal(1u, second.RecordsSkipped);
        Assert.Contains(second.Warnings, warning => warning.Contains("duplicate skipped", StringComparison.Ordinal));
        Assert.Equal("K7RND", stored.StationCallsign);
        Assert.Equal("CN87", stored.StationSnapshot.Grid);
    }

    [Fact]
    public void Import_adif_refresh_updates_existing_record_and_preserves_absent_fields()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var logged = state.LogQso(new LogQsoRequest
        {
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Rtty,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-01-15T15:23:00Z", System.Globalization.CultureInfo.InvariantCulture)),
                Comment = "Keep comment",
                Notes = "Old notes"
            }
        });

        var response = state.ImportAdif(
            Utf8("<CALL:4>W1AW\n<QSO_DATE:8>20260115\n<TIME_ON:6>152300\n<BAND:3>20M\n<MODE:4>RTTY\n<NOTES:9>New notes\n<EOR>\n"),
            refresh: true);

        var stored = state.GetQso(logged.LocalId);

        Assert.NotNull(stored);
        Assert.Equal(1u, response.RecordsUpdated);
        Assert.Contains(response.Warnings, warning => warning.Contains("refreshed existing record", StringComparison.Ordinal));
        Assert.Equal("New notes", stored!.Notes);
        Assert.Equal("Keep comment", stored.Comment);
    }

    [Fact]
    public void Import_adif_skips_invalid_time_on_length_with_warning()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var response = state.ImportAdif(
            Utf8("<CALL:4>W1AW\n<QSO_DATE:8>20260115\n<TIME_ON:1>1\n<BAND:3>20M\n<MODE:4>RTTY\n<EOR>\n"),
            refresh: false);

        Assert.Equal(0u, response.RecordsImported);
        Assert.Equal(1u, response.RecordsSkipped);
        Assert.Contains(response.Warnings, warning => warning.Contains("invalid ADIF date/time '20260115/1'. Skipped.", StringComparison.Ordinal));
    }

    [Fact]
    public void Import_adif_skips_invalid_band_with_warning()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var response = state.ImportAdif(
            Utf8("<CALL:4>W1AW\n<QSO_DATE:8>20260115\n<TIME_ON:4>1523\n<BAND:5>BOGUS\n<MODE:4>RTTY\n<EOR>\n"),
            refresh: false);

        Assert.Equal(0u, response.RecordsImported);
        Assert.Equal(1u, response.RecordsSkipped);
        Assert.Contains(response.Warnings, warning => warning.Contains("unrecognized ADIF band 'BOGUS'. Skipped.", StringComparison.Ordinal));
    }

    [Fact]
    public void Export_adif_filters_by_contest_and_orders_oldest_first()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        state.LogQso(new LogQsoRequest
        {
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1NEW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                ContestId = "WWDX",
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-01-16T01:00:00Z", System.Globalization.CultureInfo.InvariantCulture))
            }
        });
        state.LogQso(new LogQsoRequest
        {
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1OLD",
                Band = Band._20M,
                Mode = Mode.Ft8,
                ContestId = "WWDX",
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-01-15T01:00:00Z", System.Globalization.CultureInfo.InvariantCulture))
            }
        });
        state.LogQso(new LogQsoRequest
        {
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1OFF",
                Band = Band._20M,
                Mode = Mode.Ft8,
                ContestId = "STATEQP",
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-01-14T01:00:00Z", System.Globalization.CultureInfo.InvariantCulture))
            }
        });

        var payload = state.ExportAdif(new ExportAdifRequest
        {
            ContestId = "WWDX",
            IncludeHeader = true
        });
        var text = Encoding.UTF8.GetString(payload);

        Assert.Contains("<ADIF_VER:5>3.1.7", text, StringComparison.Ordinal);
        Assert.Contains("<PROGRAMID:9>QsoRipper", text, StringComparison.Ordinal);
        Assert.DoesNotContain("W1OFF", text, StringComparison.Ordinal);
        Assert.True(text.IndexOf("W1OLD", StringComparison.Ordinal) < text.IndexOf("W1NEW", StringComparison.Ordinal));
    }

    [Fact]
    public void Adif_round_trips_normalized_split_and_geo_fields()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var payload = Utf8(
            "<STATION_CALLSIGN:5>K7RND<CALL:4>W1AW<QSO_DATE:8>20260115<TIME_ON:4>1523" +
            "<BAND:3>20M<MODE:3>SSB<BAND_RX:3>40M<FREQ_RX:5>7.075" +
            "<LAT:11>N041 30.000<LON:11>W071 45.500<ALTITUDE:3>150" +
            "<GRIDSQUARE_EXT:2>ab<OWNER_CALLSIGN:4>W1AW<QSO_COMPLETE:1>Y" +
            "<APP_QSORIPPER_RX_WPM:2>28" +
            "<MY_ALTITUDE:3>550<MY_GRIDSQUARE_EXT:2>bb<EOR>\n");

        var imported = state.ImportAdif(payload, refresh: false);
        Assert.Equal(1u, imported.RecordsImported);

        var stored = state.ListQsos(new ListQsosRequest()).Single();

        Assert.Equal(Band._40M, stored.BandRx);
        Assert.Equal(7_075_000ul, stored.FrequencyRxHz);
        Assert.Equal(150.0, stored.WorkedAltitudeMeters);
        Assert.Equal("ab", stored.WorkedGridsquareExt);
        Assert.Equal("W1AW", stored.OwnerCallsign);
        Assert.Equal(QsoCompletion.Yes, stored.QsoComplete);
        Assert.True(stored.HasCwDecodeRxWpm);
        Assert.Equal(28u, stored.CwDecodeRxWpm);
        Assert.True(stored.HasWorkedLatitude);
        Assert.True(stored.HasWorkedLongitude);
        Assert.NotNull(stored.StationSnapshot);
        Assert.Equal(550.0, stored.StationSnapshot.AltitudeMeters);
        Assert.Equal("bb", stored.StationSnapshot.GridsquareExt);

        var exported = Encoding.UTF8.GetString(state.ExportAdif(new ExportAdifRequest()));

        Assert.Contains("<BAND_RX:3>40M", exported, StringComparison.Ordinal);
        Assert.Contains("<FREQ_RX:5>7.075", exported, StringComparison.Ordinal);
        Assert.Contains("<ALTITUDE:3>150", exported, StringComparison.Ordinal);
        Assert.Contains("<GRIDSQUARE_EXT:2>ab", exported, StringComparison.Ordinal);
        Assert.Contains("<OWNER_CALLSIGN:4>W1AW", exported, StringComparison.Ordinal);
        Assert.Contains("<QSO_COMPLETE:1>Y", exported, StringComparison.Ordinal);
        Assert.Contains("<APP_QSORIPPER_RX_WPM:2>28", exported, StringComparison.Ordinal);
        Assert.Contains("<MY_ALTITUDE:3>550", exported, StringComparison.Ordinal);
        Assert.Contains("<MY_GRIDSQUARE_EXT:2>bb", exported, StringComparison.Ordinal);
        Assert.Contains("<LAT:11>N041 30.000", exported, StringComparison.Ordinal);
        Assert.Contains("<LON:11>W071 45.500", exported, StringComparison.Ordinal);

        // Each new field should be emitted exactly once (no duplicates from extra_fields).
        AssertSingleAdifField(exported, "BAND_RX");
        AssertSingleAdifField(exported, "FREQ_RX");
        AssertSingleAdifField(exported, "LAT");
        AssertSingleAdifField(exported, "LON");
        AssertSingleAdifField(exported, "ALTITUDE");
        AssertSingleAdifField(exported, "GRIDSQUARE_EXT");
        AssertSingleAdifField(exported, "OWNER_CALLSIGN");
        AssertSingleAdifField(exported, "QSO_COMPLETE");
        AssertSingleAdifField(exported, "APP_QSORIPPER_RX_WPM");
        AssertSingleAdifField(exported, "MY_ALTITUDE");
        AssertSingleAdifField(exported, "MY_GRIDSQUARE_EXT");
    }

    private static void AssertSingleAdifField(string adif, string key)
    {
        var matches = System.Text.RegularExpressions.Regex.Matches(adif, $"<{key}:", System.Text.RegularExpressions.RegexOptions.IgnoreCase);
        Assert.True(matches.Count == 1, $"Expected exactly one <{key}:...> tag, found {matches.Count}");
    }

    [Fact]
    public void Update_qso_preserves_fields_not_present_in_partial_update()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            }
        });

        var logged = state.LogQso(new LogQsoRequest
        {
            SyncToQrz = false,
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-06-01T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture)),
                RstSent = new RstReport { Raw = "59" },
                RstReceived = new RstReport { Raw = "57" },
                Notes = "Initial notes",
                FrequencyHz = 14_074_000,
            }
        });

        // Update with only Comment changed — all other fields should be preserved.
        var updateResponse = state.UpdateQso(new UpdateQsoRequest
        {
            SyncToQrz = false,
            Qso = new QsoRecord
            {
                LocalId = logged.LocalId,
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.Parse("2026-06-01T12:00:00Z", System.Globalization.CultureInfo.InvariantCulture)),
                Comment = "Updated comment",
            }
        });

        var stored = state.GetQso(logged.LocalId);

        Assert.True(updateResponse.Success);
        Assert.NotNull(stored);
        Assert.Equal("Updated comment", stored!.Comment);
        Assert.Equal("59", stored.RstSent?.Raw);
        Assert.Equal("57", stored.RstReceived?.Raw);
        Assert.Equal("Initial notes", stored.Notes);
        Assert.Equal(14_074_000UL, stored.FrequencyHz);
    }

    [Fact]
    public async Task Import_adif_grpc_converts_post_await_validation_errors_to_invalid_argument()
    {
        var state = CreateState();
        var service = new ManagedLogbookGrpcService(state);
        var stream = new TestAsyncStreamReader<ImportAdifRequest>([
            new ImportAdifRequest()
        ]);

        var ex = await Assert.ThrowsAsync<RpcException>(
            () => service.ImportAdif(stream, new TestServerCallContext()));

        Assert.Equal(StatusCode.InvalidArgument, ex.StatusCode);
        Assert.Equal("chunk is required.", ex.Status.Detail);
    }

    public void Dispose()
    {
        if (Directory.Exists(_tempDirectory))
        {
            Directory.Delete(_tempDirectory, recursive: true);
        }
    }

    [Fact]
    public void Soft_delete_marks_row_with_tombstone_and_keeps_it_retrievable()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var loggedResp = LogSampleQso(state, "W1AW");
        var logged = state.GetQso(loggedResp.LocalId)!;

        var outcome = state.DeleteQso(logged.LocalId, queueRemoteDelete: false);

        Assert.True(outcome.Found);
        Assert.False(outcome.RemoteDeleteQueued);

        var fetched = state.GetQso(logged.LocalId);
        Assert.NotNull(fetched);
        Assert.NotNull(fetched!.DeletedAt);
        Assert.False(fetched.PendingRemoteDelete);
    }

    [Fact]
    public void Soft_delete_with_qrz_logid_queues_remote_delete()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "test-api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87",
            },
        });
        var loggedResp = state.LogQso(new LogQsoRequest
        {
            SyncToQrz = true,
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
            },
        });
        Assert.False(string.IsNullOrEmpty(loggedResp.QrzLogid));
        var logged = state.GetQso(loggedResp.LocalId)!;

        var outcome = state.DeleteQso(logged.LocalId, queueRemoteDelete: true);

        Assert.True(outcome.Found);
        Assert.True(outcome.RemoteDeleteQueued);
        Assert.False(outcome.MissingQrzLogid);

        var fetched = state.GetQso(logged.LocalId);
        Assert.NotNull(fetched!.DeletedAt);
        Assert.True(fetched.PendingRemoteDelete);
    }

    [Fact]
    public void Soft_delete_without_qrz_logid_reports_missing_logid_when_remote_requested()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var loggedResp = LogSampleQso(state, "W1AW");
        var logged = state.GetQso(loggedResp.LocalId)!;

        var outcome = state.DeleteQso(logged.LocalId, queueRemoteDelete: true);

        Assert.True(outcome.Found);
        Assert.False(outcome.RemoteDeleteQueued);
        Assert.True(outcome.MissingQrzLogid);
    }

    [Fact]
    public void Update_on_soft_deleted_row_throws_QsoSoftDeletedException()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var loggedResp = LogSampleQso(state, "W1AW");
        var logged = state.GetQso(loggedResp.LocalId)!;
        state.DeleteQso(logged.LocalId, queueRemoteDelete: false);

        Assert.Throws<QsoSoftDeletedException>(() => state.UpdateQso(new UpdateQsoRequest
        {
            Qso = new QsoRecord(logged) { Notes = "should not apply" },
        }));
    }

    [Fact]
    public void Restore_clears_tombstone_and_pending_flag()
    {
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "test-api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87",
            },
        });
        var loggedResp = state.LogQso(new LogQsoRequest
        {
            SyncToQrz = true,
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
            },
        });
        var logged = state.GetQso(loggedResp.LocalId)!;
        var originalLogid = logged.QrzLogid;
        Assert.False(string.IsNullOrEmpty(originalLogid));
        state.DeleteQso(logged.LocalId, queueRemoteDelete: true);

        var outcome = state.RestoreQso(logged.LocalId);

        Assert.True(outcome.Found);
        Assert.NotNull(outcome.Restored);
        Assert.Null(outcome.Restored!.DeletedAt);
        Assert.False(outcome.Restored.PendingRemoteDelete);
        Assert.Equal(originalLogid, outcome.Restored.QrzLogid);
        Assert.Equal(SyncStatus.Synced, outcome.Restored.SyncStatus);
    }

    [Fact]
    public void Restore_unknown_local_id_returns_not_found()
    {
        var state = CreateState();

        var outcome = state.RestoreQso("does-not-exist");

        Assert.False(outcome.Found);
        Assert.Null(outcome.Restored);
    }

    [Fact]
    public void List_qsos_excludes_soft_deleted_by_default()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var keepResp = LogSampleQso(state, "W1AW");
        var keep = state.GetQso(keepResp.LocalId)!;
        var trashResp = LogSampleQso(state, "K7RND");
        var trash = state.GetQso(trashResp.LocalId)!;
        state.DeleteQso(trash.LocalId, queueRemoteDelete: false);

        var active = state.ListQsos(new ListQsosRequest());

        Assert.Contains(active, q => q.LocalId == keep.LocalId);
        Assert.DoesNotContain(active, q => q.LocalId == trash.LocalId);
    }

    [Fact]
    public void List_qsos_with_deleted_only_filter_returns_trash()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var keepResp = LogSampleQso(state, "W1AW");
        var keep = state.GetQso(keepResp.LocalId)!;
        var trashResp = LogSampleQso(state, "K7RND");
        var trash = state.GetQso(trashResp.LocalId)!;
        state.DeleteQso(trash.LocalId, queueRemoteDelete: false);

        var deleted = state.ListQsos(new ListQsosRequest { DeletedFilter = DeletedRecordsFilter.DeletedOnly });

        Assert.DoesNotContain(deleted, q => q.LocalId == keep.LocalId);
        Assert.Contains(deleted, q => q.LocalId == trash.LocalId);
    }

    [Fact]
    public async Task Restore_qso_grpc_returns_restored_record()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var loggedResp = LogSampleQso(state, "W1AW");
        var logged = state.GetQso(loggedResp.LocalId)!;
        state.DeleteQso(logged.LocalId, queueRemoteDelete: false);

        var service = new ManagedLogbookGrpcService(state);
        var response = await service.RestoreQso(
            new RestoreQsoRequest { LocalId = logged.LocalId },
            null!);

        Assert.True(response.Success);
        Assert.NotNull(response.Restored);
        Assert.Null(response.Restored!.DeletedAt);
    }

    [Fact]
    public async Task Restore_qso_grpc_throws_not_found_for_unknown_id()
    {
        var state = CreateState();
        var service = new ManagedLogbookGrpcService(state);

        var ex = await Assert.ThrowsAsync<RpcException>(() => service.RestoreQso(
            new RestoreQsoRequest { LocalId = "missing-id" },
            null!));
        Assert.Equal(StatusCode.NotFound, ex.StatusCode);
    }

    [Fact]
    public async Task Update_qso_grpc_on_soft_deleted_row_returns_failed_precondition()
    {
        var state = CreateState();
        EnsureStationConfigured(state);
        var loggedResp = LogSampleQso(state, "W1AW");
        var logged = state.GetQso(loggedResp.LocalId)!;
        state.DeleteQso(logged.LocalId, queueRemoteDelete: false);

        var service = new ManagedLogbookGrpcService(state);
        var ex = await Assert.ThrowsAsync<RpcException>(() => service.UpdateQso(
            new UpdateQsoRequest
            {
                Qso = new QsoRecord(logged) { Notes = "blocked" },
            },
            null!));
        Assert.Equal(StatusCode.FailedPrecondition, ex.StatusCode);
    }

    private static LogQsoResponse LogSampleQso(ManagedEngineState state, string callsign)
    {
        return state.LogQso(new LogQsoRequest
        {
            SyncToQrz = false,
            Qso = new QsoRecord
            {
                WorkedCallsign = callsign,
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
            },
        });
    }

    [Fact]
    public void SyncWithQrz_populates_remote_deletes_pushed_counter()
    {
        var state = CreateStateWithSync();
        state.SaveSetup(new SaveSetupRequest
        {
            QrzLogbookApiKey = "api-key",
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87",
            },
        });
        var loggedResp = state.LogQso(new LogQsoRequest
        {
            SyncToQrz = false,
            Qso = new QsoRecord
            {
                WorkedCallsign = "W1AW",
                Band = Band._20M,
                Mode = Mode.Ft8,
                UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
            },
        });
        state.SyncWithQrz();
        var synced = state.GetQso(loggedResp.LocalId)!;
        Assert.False(string.IsNullOrEmpty(synced.QrzLogid));

        state.DeleteQso(synced.LocalId, queueRemoteDelete: true);
        var secondSync = state.SyncWithQrz();

        Assert.True(secondSync.Complete);
        Assert.Equal(1u, secondSync.RemoteDeletesPushed);
        Assert.Equal(0u, secondSync.DeletesSkippedRemote);
    }

    private static void EnsureStationConfigured(ManagedEngineState state)
    {
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87",
            },
        });
    }

    private ManagedEngineState CreateState()
    {
        return new ManagedEngineState(Path.Combine(_tempDirectory, "config.toml"), new MemoryStorage());
    }

    private ManagedEngineState CreateStateWithSync()
    {
        var storage = new MemoryStorage();
        var fakeApi = new FakeQrzLogbookApi();
        var syncEngine = new QrzSyncEngine(fakeApi);
        return new ManagedEngineState(
            Path.Combine(_tempDirectory, "config.toml"),
            storage,
            lookupCoordinator: null,
            rigControlMonitor: null,
            spaceWeatherMonitor: null,
            syncEngine: syncEngine);
    }

    private ManagedEngineState CreateStateWithRigSnapshot(RigSnapshot snapshot)
    {
        var storage = new MemoryStorage();
        var monitor = new RigControlMonitor(
            new FakeRigControlProvider(() => snapshot.Clone()),
            TimeSpan.Zero);
        return new ManagedEngineState(
            Path.Combine(_tempDirectory, "config.toml"),
            storage,
            lookupCoordinator: null,
            rigControlMonitor: monitor,
            spaceWeatherMonitor: null,
            syncEngine: null);
    }

    private static byte[] Utf8(string value)
    {
        return Encoding.UTF8.GetBytes(value);
    }

    private static HttpClient GetOwnedSyncHttpClient(QrzSyncEngine syncEngine)
    {
        var api = GetRequiredPrivateFieldValue(syncEngine, "_client");
        return GetRequiredPrivateField<HttpClient>(api, "_httpClient");
    }

    private static bool IsHttpClientDisposed(HttpClient client)
    {
        var disposedField = typeof(HttpMessageInvoker).GetField("_disposed", BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new InvalidOperationException("Could not locate HttpMessageInvoker._disposed.");

        return disposedField.GetValue(client) is true;
    }

    private static T GetRequiredPrivateField<T>(object instance, string fieldName)
        where T : class
    {
        return Assert.IsType<T>(GetRequiredPrivateFieldValue(instance, fieldName));
    }

    private static object GetRequiredPrivateFieldValue(object instance, string fieldName)
    {
        ArgumentNullException.ThrowIfNull(instance);
        ArgumentException.ThrowIfNullOrWhiteSpace(fieldName);

        var field = instance.GetType().GetField(fieldName, BindingFlags.Instance | BindingFlags.NonPublic)
            ?? throw new InvalidOperationException(
                $"Could not locate field '{fieldName}' on {instance.GetType().FullName}.");

        return field.GetValue(instance)
            ?? throw new InvalidOperationException(
                $"Field '{fieldName}' on {instance.GetType().FullName} was null.");
    }

    private sealed class FakeRigControlProvider(Func<RigSnapshot> factory) : IRigControlProvider
    {
        public RigSnapshot GetSnapshot() => factory();
    }

    private sealed class TestAsyncStreamReader<T>(IReadOnlyList<T> items)
        : IAsyncStreamReader<T>
    {
        private int _index = -1;

        public T Current => items[_index];

        public Task<bool> MoveNext(CancellationToken cancellationToken)
        {
            cancellationToken.ThrowIfCancellationRequested();
            _index++;
            return Task.FromResult(_index < items.Count);
        }
    }

    private sealed class TestServerCallContext : ServerCallContext
    {
        private readonly Metadata _responseTrailers = [];
        private readonly Dictionary<object, object> _userState = [];
        private WriteOptions? _writeOptions;
        private Status _status;

        protected override string MethodCore => "test";
        protected override string HostCore => "localhost";
        protected override string PeerCore => "test-peer";
        protected override DateTime DeadlineCore => DateTime.UtcNow.AddMinutes(1);
        protected override Metadata RequestHeadersCore => [];
        protected override CancellationToken CancellationTokenCore => CancellationToken.None;
        protected override Metadata ResponseTrailersCore => _responseTrailers;
        protected override Status StatusCore { get => _status; set => _status = value; }
        protected override WriteOptions? WriteOptionsCore { get => _writeOptions; set => _writeOptions = value; }
        protected override AuthContext AuthContextCore => new("none", []);

        protected override ContextPropagationToken CreatePropagationTokenCore(ContextPropagationOptions? options) =>
            throw new NotSupportedException();

        protected override Task WriteResponseHeadersAsyncCore(Metadata responseHeaders) => Task.CompletedTask;

        protected override IDictionary<object, object> UserStateCore => _userState;
    }

    /// <summary>
    /// Minimal in-memory fake for <see cref="IQrzLogbookApi"/> that records uploads and returns empty fetches.
    /// </summary>
    private sealed class FakeQrzLogbookApi : IQrzLogbookApi
    {
        private int _logIdCounter;

        public Task<List<QsoRecord>> FetchQsosAsync(string? sinceDateYmd) =>
            Task.FromResult(new List<QsoRecord>());

        public Task<string> UploadQsoAsync(QsoRecord qso, string? bookOwner = null)
        {
            var logId = $"FAKE-{Interlocked.Increment(ref _logIdCounter)}";
            return Task.FromResult(logId);
        }

        public Task<string> UploadQsoWithReplaceAsync(QsoRecord qso, string? bookOwner = null)
        {
            var logId = $"FAKE-{Interlocked.Increment(ref _logIdCounter)}";
            return Task.FromResult(logId);
        }

        public Task<string> UpdateQsoAsync(QsoRecord qso, string? bookOwner = null)
        {
            var logId = $"FAKE-{Interlocked.Increment(ref _logIdCounter)}";
            return Task.FromResult(logId);
        }

        public Task<QrzLogbookStatus> GetStatusAsync() =>
            Task.FromResult(new QrzLogbookStatus("K7RND", (uint)_logIdCounter));

        public Task DeleteQsoAsync(string logid) => Task.CompletedTask;
    }

    private sealed class FakeMalformedQrzLogbookApi : IQrzLogbookApi
    {
        public Task<List<QsoRecord>> FetchQsosAsync(string? sinceDateYmd)
            => Task.FromResult(new List<QsoRecord> { null! });

        public Task<string> UploadQsoAsync(QsoRecord qso, string? bookOwner = null) => Task.FromResult("FAKE-1");

        public Task<string> UploadQsoWithReplaceAsync(QsoRecord qso, string? bookOwner = null) => Task.FromResult("FAKE-1");

        public Task<string> UpdateQsoAsync(QsoRecord qso, string? bookOwner = null) => Task.FromResult("FAKE-1");

        public Task<QrzLogbookStatus> GetStatusAsync() =>
            Task.FromResult(new QrzLogbookStatus("K7RND", 0));

        public Task DeleteQsoAsync(string logid) => Task.CompletedTask;
    }
}
#pragma warning restore CA1707
