using System.Text;
using Google.Protobuf.WellKnownTypes;
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
        var persistedJson = File.ReadAllText(Path.Combine(_tempDirectory, "managed-engine.json"));

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
        Assert.DoesNotContain("\"logFilePath\"", persistedJson, StringComparison.OrdinalIgnoreCase);
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
        Assert.Equal(1u, afterSync.QrzQsoCount);
        Assert.Equal("K7RND", afterSync.QrzLogbookOwner);
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

        Assert.Equal("The managed .NET engine currently supports only memory storage.", exception.Message);
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
        var state = CreateState();
        state.SaveSetup(new SaveSetupRequest
        {
            StationProfile = new StationProfile
            {
                ProfileName = "Home",
                StationCallsign = "K7RND",
                OperatorCallsign = "K7RND",
                Grid = "CN87"
            },
            RigControl = new RigControlSettings
            {
                Enabled = true,
                Host = "127.0.0.1",
                Port = 4532
            }
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

    public void Dispose()
    {
        if (Directory.Exists(_tempDirectory))
        {
            Directory.Delete(_tempDirectory, recursive: true);
        }
    }

    private ManagedEngineState CreateState()
    {
        return new ManagedEngineState(Path.Combine(_tempDirectory, "managed-engine.json"), new MemoryStorage());
    }

    private ManagedEngineState CreateStateWithSync()
    {
        var storage = new MemoryStorage();
        var fakeApi = new FakeQrzLogbookApi();
        var syncEngine = new QrzSyncEngine(fakeApi);
        return new ManagedEngineState(
            Path.Combine(_tempDirectory, "managed-engine.json"),
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
            Path.Combine(_tempDirectory, "managed-engine.json"),
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

    private sealed class FakeRigControlProvider(Func<RigSnapshot> factory) : IRigControlProvider
    {
        public RigSnapshot GetSnapshot() => factory();
    }

    /// <summary>
    /// Minimal in-memory fake for <see cref="IQrzLogbookApi"/> that records uploads and returns empty fetches.
    /// </summary>
    private sealed class FakeQrzLogbookApi : IQrzLogbookApi
    {
        private int _logIdCounter;

        public Task<List<QsoRecord>> FetchQsosAsync(string? sinceDateYmd) =>
            Task.FromResult(new List<QsoRecord>());

        public Task<string> UploadQsoAsync(QsoRecord qso)
        {
            var logId = $"FAKE-{Interlocked.Increment(ref _logIdCounter)}";
            return Task.FromResult(logId);
        }
    }
}
#pragma warning restore CA1707
