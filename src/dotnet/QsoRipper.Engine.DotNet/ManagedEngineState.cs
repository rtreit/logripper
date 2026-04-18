using System.Text.RegularExpressions;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Engine.Lookup;
using QsoRipper.Engine.QrzLogbook;
using QsoRipper.Engine.RigControl;
using QsoRipper.Engine.SpaceWeather;
using QsoRipper.Engine.Storage;
using QsoRipper.Engine.Storage.Memory;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Engine.DotNet;

internal sealed class ManagedEngineState
{
    private const string PersistenceStepDescription = "The managed .NET engine keeps its logbook in memory. No persistence input is required during setup.";
    private const string PersistenceStepDescriptionSqlite = "The managed .NET engine stores its logbook in a local SQLite database backed by shared setup.";
    private const string PersistenceStepLabel = "Storage";
    private const string InMemoryPersistenceSummary = "In-memory logbook";
    private const string SqlitePersistenceSummary = "SQLite logbook";

    private const string StorageBackendKey = "QSORIPPER_STORAGE_BACKEND";
    private const string QrzXmlUsernameKey = "QSORIPPER_QRZ_XML_USERNAME";
    private const string QrzXmlPasswordKey = "QSORIPPER_QRZ_XML_PASSWORD";
    private const string QrzUserAgentKey = "QSORIPPER_QRZ_USER_AGENT";
    private const string QrzLogbookApiKeyKey = "QSORIPPER_QRZ_LOGBOOK_API_KEY";
    private const string RigEnabledKey = "QSORIPPER_RIGCTLD_ENABLED";

    private const string ManagedLookupProviderSummary = "Managed sample provider";

    private static readonly JsonFormatter ProtoJsonFormatter = new(JsonFormatter.Settings.Default.WithFormatDefaultValues(true));
    private static readonly JsonParser ProtoJsonParser = new(JsonParser.Settings.Default.WithIgnoreUnknownFields(true));

    private readonly Lock _gate = new();
    private readonly IEngineStorage _storage;
    private ILookupCoordinator _lookupCoordinator;
    private readonly RigControlMonitor? _rigControlMonitor;
    private readonly SpaceWeatherMonitor? _spaceWeatherMonitor;
    private readonly string _configPath;
    private readonly SharedPersistedSetupConfig _persistedSetup;
    private readonly string? _currentPersistenceLocation;
    private string? _qrzXmlUsername;
    private string? _qrzXmlPassword;
    private bool _hasQrzXmlPassword;
    private string? _qrzLogbookApiKey;
    private bool _hasQrzLogbookApiKey;
    private SyncConfig _syncConfig;
    private RigControlSettings? _rigControl;
    private readonly List<ManagedPersistedStationProfile> _stationProfiles;
    private string? _activeProfileId;
    private StationProfile? _sessionOverrideProfile;
    private readonly Dictionary<string, string> _runtimeOverrides;
    private readonly bool _ownsSyncEngine;
    private QrzLogbookClient? _ownedSyncClient;
    private QrzSyncEngine? _syncEngine;

    public ManagedEngineState(string configPath)
        : this(configPath, new MemoryStorage(), null, null, null, null)
    {
    }

    public ManagedEngineState(string configPath, IEngineStorage storage)
        : this(configPath, storage, null, null, null, null)
    {
    }

    public ManagedEngineState(string configPath, IEngineStorage storage, ILookupCoordinator? lookupCoordinator)
        : this(configPath, storage, lookupCoordinator, null, null, null)
    {
    }

    public ManagedEngineState(string configPath, IEngineStorage storage, ILookupCoordinator? lookupCoordinator, RigControlMonitor? rigControlMonitor)
        : this(configPath, storage, lookupCoordinator, rigControlMonitor, null, null)
    {
    }

    public ManagedEngineState(string configPath, IEngineStorage storage, ILookupCoordinator? lookupCoordinator, RigControlMonitor? rigControlMonitor, SpaceWeatherMonitor? spaceWeatherMonitor)
        : this(configPath, storage, lookupCoordinator, rigControlMonitor, spaceWeatherMonitor, null, null, null)
    {
    }

    public ManagedEngineState(string configPath, IEngineStorage storage, ILookupCoordinator? lookupCoordinator, RigControlMonitor? rigControlMonitor, SpaceWeatherMonitor? spaceWeatherMonitor, QrzSyncEngine? syncEngine)
        : this(configPath, storage, lookupCoordinator, rigControlMonitor, spaceWeatherMonitor, syncEngine, null, null)
    {
    }

    public ManagedEngineState(string configPath, IEngineStorage storage, ILookupCoordinator? lookupCoordinator, RigControlMonitor? rigControlMonitor, SpaceWeatherMonitor? spaceWeatherMonitor, QrzSyncEngine? syncEngine, string? currentPersistenceLocation, LoadedSharedSetupConfig? loadedPersistedSetup)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(configPath);
        ArgumentNullException.ThrowIfNull(storage);

        _configPath = Path.GetFullPath(configPath.Trim());
        _storage = storage;
        _lookupCoordinator = lookupCoordinator ?? CreateDefaultCoordinator(storage);
        _rigControlMonitor = rigControlMonitor;
        _spaceWeatherMonitor = spaceWeatherMonitor;
        _ownsSyncEngine = syncEngine is null;
        _syncEngine = syncEngine;
        var loadedSetup = loadedPersistedSetup ?? SharedSetupConfigPersistence.Load(_configPath);
        _persistedSetup = loadedSetup.Config;
        _currentPersistenceLocation = NormalizeOptional(currentPersistenceLocation)
            ?? (_storage.BackendName.Equals("sqlite", StringComparison.OrdinalIgnoreCase)
                ? NormalizeOptional(_persistedSetup.GetPersistedLogFilePath())
                : null);
        _qrzXmlUsername = NormalizeOptional(_persistedSetup.QrzXmlUsername);
        _qrzXmlPassword = NormalizeOptional(_persistedSetup.QrzXmlPassword);
        _hasQrzXmlPassword = _qrzXmlPassword is not null;
        _qrzLogbookApiKey = NormalizeOptional(_persistedSetup.QrzLogbookApiKey);
        _hasQrzLogbookApiKey = _qrzLogbookApiKey is not null;
        _syncConfig = _persistedSetup.SyncConfig.Clone();
        _rigControl = _persistedSetup.RigControl?.Clone();
        _stationProfiles = _persistedSetup.StationProfiles
            .Select(static entry => new ManagedPersistedStationProfile
            {
                ProfileId = entry.ProfileId,
                ProfileJson = entry.ProfileJson,
            })
            .ToList();
        _activeProfileId = NormalizeOptional(_persistedSetup.ActiveProfileId);
        _sessionOverrideProfile = null;
        _runtimeOverrides = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

        if (lookupCoordinator is null)
        {
            RebuildLookupCoordinatorNoLock();
        }

        if (_ownsSyncEngine)
        {
            RebuildSyncEngineNoLock();
        }

        if (loadedSetup.LastSyncUtc is { } lastSync)
        {
            Sync(_storage.Logbook.UpsertSyncMetadataAsync(new SyncMetadata { LastSync = lastSync }));
        }
    }

    public static EngineInfo BuildEngineInfo()
    {
        return new EngineInfo
        {
            EngineId = EngineCatalog.DotNetProfile.EngineId,
            DisplayName = EngineCatalog.DotNetProfile.DisplayName,
            Version = typeof(ManagedEngineState).Assembly.GetName().Version?.ToString() ?? "0.0.0",
            Capabilities =
            {
                "engine-info",
                "logbook",
                "lookup-cache",
                "lookup-callsign",
                "lookup-stream",
                "setup",
                "station-profiles",
                "runtime-config",
                "rig-control",
                "space-weather",
            }
        };
    }

    public SetupStatus GetSetupStatus()
    {
        lock (_gate)
        {
            return BuildSetupStatusNoLock();
        }
    }

    public GetSetupWizardStateResponse GetSetupWizardState()
    {
        lock (_gate)
        {
            var response = new GetSetupWizardStateResponse
            {
                Status = BuildSetupStatusNoLock(),
            };
            response.Steps.AddRange(BuildStepStatusesNoLock());
            response.StationProfiles.AddRange(BuildStationProfileRecordsNoLock());
            return response;
        }
    }

    public static ValidateSetupStepResponse ValidateSetupStep(ValidateSetupStepRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        var response = new ValidateSetupStepResponse();
        switch (request.Step)
        {
            case SetupWizardStep.LogFile:
                response.Valid = true;
                break;
            case SetupWizardStep.StationProfiles:
                var profile = request.StationProfile ?? new StationProfile();
                AddValidation(response, "profile_name", !string.IsNullOrWhiteSpace(profile.ProfileName), "Profile name is required.");
                AddValidation(response, "callsign", !string.IsNullOrWhiteSpace(profile.StationCallsign), "Station callsign is required.");
                AddValidation(response, "operator_callsign", !string.IsNullOrWhiteSpace(profile.OperatorCallsign), "Operator callsign is required.");
                AddValidation(response, "grid_square", !string.IsNullOrWhiteSpace(profile.Grid), "Grid square is required.");
                break;
            case SetupWizardStep.QrzIntegration:
                var hasUsername = !string.IsNullOrWhiteSpace(request.QrzXmlUsername);
                var hasPassword = !string.IsNullOrWhiteSpace(request.QrzXmlPassword);
                AddValidation(response, "qrz_xml_username", hasUsername == hasPassword, "Provide both username and password, or leave both blank.");
                AddValidation(response, "qrz_xml_password", hasUsername == hasPassword, "Provide both username and password, or leave both blank.");
                break;
        }

        response.Valid = response.Fields.All(field => field.Valid);
        return response;
    }

    public static TestQrzCredentialsResponse TestQrzCredentials(string username, string password)
    {
        if (string.IsNullOrWhiteSpace(username) || string.IsNullOrWhiteSpace(password))
        {
            return new TestQrzCredentialsResponse
            {
                Success = false,
                ErrorMessage = "Username and password are required.",
            };
        }

        if (LooksRejected(username) || LooksRejected(password))
        {
            return new TestQrzCredentialsResponse
            {
                Success = false,
                ErrorMessage = "Managed engine rejected the supplied QRZ XML credentials.",
            };
        }

        return new TestQrzCredentialsResponse { Success = true };
    }

    public TestQrzLogbookCredentialsResponse TestQrzLogbookCredentials(string apiKey)
    {
        if (string.IsNullOrWhiteSpace(apiKey))
        {
            return new TestQrzLogbookCredentialsResponse
            {
                Success = false,
                ErrorMessage = "API key is required.",
            };
        }

        if (LooksRejected(apiKey))
        {
            return new TestQrzLogbookCredentialsResponse
            {
                Success = false,
                ErrorMessage = "Managed engine rejected the supplied QRZ logbook API key.",
            };
        }

        lock (_gate)
        {
            var counts = Sync(_storage.Logbook.GetCountsAsync());
            var response = new TestQrzLogbookCredentialsResponse
            {
                Success = true,
                QsoCount = (uint)counts.LocalQsoCount,
            };
            var active = GetEffectiveActiveProfileNoLock();
            if (!string.IsNullOrWhiteSpace(active?.StationCallsign))
            {
                response.LogbookOwner = active.StationCallsign;
            }

            return response;
        }
    }

    public SaveSetupResponse SaveSetup(SaveSetupRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            if (request.StationProfile is not null)
            {
                SaveStationProfileNoLock(
                    NormalizeProfileIdOrDefault(request.StationProfile.ProfileName, request.StationProfile.StationCallsign),
                    request.StationProfile,
                    makeActive: true);
            }

            if (!string.IsNullOrWhiteSpace(request.QrzXmlUsername))
            {
                _qrzXmlUsername = request.QrzXmlUsername.Trim();
                _persistedSetup.QrzXmlUsername = _qrzXmlUsername;
                _runtimeOverrides.Remove(QrzXmlUsernameKey);
                RebuildLookupCoordinatorNoLock();
            }

            if (!string.IsNullOrWhiteSpace(request.QrzXmlPassword))
            {
                _qrzXmlPassword = request.QrzXmlPassword.Trim();
                _hasQrzXmlPassword = true;
                _persistedSetup.QrzXmlPassword = _qrzXmlPassword;
                _runtimeOverrides.Remove(QrzXmlPasswordKey);
                RebuildLookupCoordinatorNoLock();
            }

            if (!string.IsNullOrWhiteSpace(request.QrzLogbookApiKey))
            {
                _qrzLogbookApiKey = request.QrzLogbookApiKey.Trim();
                _hasQrzLogbookApiKey = true;
                _persistedSetup.QrzLogbookApiKey = _qrzLogbookApiKey;
                _runtimeOverrides.Remove(QrzLogbookApiKeyKey);
                if (_ownsSyncEngine)
                {
                    RebuildSyncEngineNoLock();
                }
            }

            if (request.SyncConfig is not null)
            {
                _syncConfig = request.SyncConfig.Clone();
                _persistedSetup.SyncConfig = _syncConfig.Clone();
            }

            if (request.RigControl is not null)
            {
                _rigControl = request.RigControl.Clone();
                _persistedSetup.RigControl = _rigControl.Clone();
            }

            UpdatePersistedStorageSettingsNoLock(request);
            SyncPersistedProfilesNoLock();
            _runtimeOverrides.Remove(StorageBackendKey);

            PersistNoLock();
            return new SaveSetupResponse
            {
                Status = BuildSetupStatusNoLock(),
            };
        }
    }

    public ListStationProfilesResponse ListStationProfiles()
    {
        lock (_gate)
        {
            var response = new ListStationProfilesResponse();
            response.Profiles.AddRange(BuildStationProfileRecordsNoLock());
            if (!string.IsNullOrWhiteSpace(_activeProfileId))
            {
                response.ActiveProfileId = _activeProfileId;
            }

            return response;
        }
    }

    public StationProfileRecord? GetStationProfile(string profileId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        lock (_gate)
        {
            var stored = _stationProfiles.FirstOrDefault(entry => string.Equals(entry.ProfileId, profileId.Trim(), StringComparison.Ordinal));
            if (stored is null)
            {
                return null;
            }

            return BuildStationProfileRecordNoLock(stored);
        }
    }

    public SaveStationProfileResponse SaveStationProfile(SaveStationProfileRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            var profile = request.Profile ?? throw new InvalidOperationException("profile is required.");
            var profileId = NormalizeProfileIdOrDefault(request.ProfileId, profile.ProfileName, profile.StationCallsign);
            var record = SaveStationProfileNoLock(profileId, profile, request.MakeActive);
            SyncPersistedProfilesNoLock();
            PersistNoLock();

            var response = new SaveStationProfileResponse
            {
                Profile = record,
            };
            if (!string.IsNullOrWhiteSpace(_activeProfileId))
            {
                response.ActiveProfileId = _activeProfileId;
            }

            return response;
        }
    }

    public StationProfileRecord? SetActiveStationProfile(string profileId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        lock (_gate)
        {
            var stored = _stationProfiles.FirstOrDefault(entry => string.Equals(entry.ProfileId, profileId.Trim(), StringComparison.Ordinal));
            if (stored is null)
            {
                return null;
            }

            _activeProfileId = stored.ProfileId;
            SyncPersistedProfilesNoLock();
            PersistNoLock();
            return BuildStationProfileRecordNoLock(stored);
        }
    }

    public bool DeleteStationProfile(string profileId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(profileId);

        lock (_gate)
        {
            if (string.Equals(_activeProfileId, profileId.Trim(), StringComparison.Ordinal))
            {
                return false;
            }

            var removed = _stationProfiles.RemoveAll(entry => string.Equals(entry.ProfileId, profileId.Trim(), StringComparison.Ordinal));
            if (removed > 0)
            {
                SyncPersistedProfilesNoLock();
                PersistNoLock();
                return true;
            }

            return false;
        }
    }

    public ActiveStationContext GetActiveStationContext()
    {
        lock (_gate)
        {
            return BuildActiveStationContextNoLock();
        }
    }

    public ActiveStationContext SetSessionStationProfileOverride(StationProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        lock (_gate)
        {
            _sessionOverrideProfile = profile.Clone();
            return BuildActiveStationContextNoLock();
        }
    }

    public ActiveStationContext ClearSessionStationProfileOverride()
    {
        lock (_gate)
        {
            _sessionOverrideProfile = null;
            return BuildActiveStationContextNoLock();
        }
    }

    public LogQsoResponse LogQso(LogQsoRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            var qso = request.Qso?.Clone() ?? throw new InvalidOperationException("qso is required.");
            ApplyStationContextNoLock(qso);
            ManagedQsoParity.NormalizeQsoForPersistence(qso);
            ValidateQsoNoLock(qso);
            FinalizeQsoForWrite(qso, isNew: true);

            var response = new LogQsoResponse
            {
                LocalId = qso.LocalId,
            };

            ApplySyncFlagsNoLock(qso, request.SyncToQrz, response);
            Sync(_storage.Logbook.InsertQsoAsync(qso));
            return response;
        }
    }

    public UpdateQsoResponse UpdateQso(UpdateQsoRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            var qso = request.Qso?.Clone() ?? throw new InvalidOperationException("qso is required.");
            if (string.IsNullOrWhiteSpace(qso.LocalId))
            {
                return new UpdateQsoResponse { Success = false, Error = "local_id is required." };
            }

            var existing = Sync(_storage.Logbook.GetQsoAsync(qso.LocalId));
            if (existing is null)
            {
                return new UpdateQsoResponse { Success = false, Error = $"QSO '{qso.LocalId}' was not found." };
            }

            // Merge incoming partial record into existing so unspecified fields are preserved.
            var merged = ManagedQsoParity.MergeQsoForUpdate(existing, qso);

            ApplyStationContextNoLock(merged, existing);
            ManagedQsoParity.NormalizeQsoForPersistence(merged);
            ValidateQsoNoLock(merged);
            FinalizeQsoForWrite(merged, isNew: false);

            var response = new UpdateQsoResponse { Success = true };
            ApplySyncFlagsNoLock(merged, request.SyncToQrz, response);
            Sync(_storage.Logbook.UpdateQsoAsync(merged));
            return response;
        }
    }

    public bool DeleteQso(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_gate)
        {
            return Sync(_storage.Logbook.DeleteQsoAsync(localId.Trim()));
        }
    }

    public QsoRecord? GetQso(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_gate)
        {
            return Sync(_storage.Logbook.GetQsoAsync(localId.Trim()));
        }
    }

    public IReadOnlyList<QsoRecord> ListQsos(ListQsosRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            var storageQuery = new QsoListQuery
            {
                After = request.After is not null ? request.After.ToDateTimeOffset() : null,
                Before = request.Before is not null ? request.Before.ToDateTimeOffset() : null,
                CallsignFilter = NormalizeOptional(request.CallsignFilter),
                BandFilter = request.HasBandFilter ? request.BandFilter : null,
                ModeFilter = request.HasModeFilter ? request.ModeFilter : null,
                ContestId = NormalizeOptional(request.ContestId),
                Offset = request.Offset > 0 ? (int)request.Offset : 0,
                Limit = request.Limit > 0 ? (int)request.Limit : null,
                Sort = request.Sort == QsoRipper.Services.QsoSortOrder.OldestFirst
                    ? Storage.QsoSortOrder.OldestFirst
                    : Storage.QsoSortOrder.NewestFirst,
            };

            return Sync(_storage.Logbook.ListQsosAsync(storageQuery));
        }
    }

    public SyncWithQrzResponse SyncWithQrz(bool fullSync = false)
    {
        lock (_gate)
        {
            if (_syncEngine is null)
            {
                return new SyncWithQrzResponse
                {
                    Complete = true,
                    Error = "QRZ logbook is not configured.",
                };
            }

            try
            {
                var result = Sync(_syncEngine.ExecuteSyncAsync(_storage.Logbook, fullSync));

                var syncResponse = new SyncWithQrzResponse
                {
                    DownloadedRecords = result.DownloadedCount,
                    UploadedRecords = result.UploadedCount,
                    ConflictRecords = result.ConflictCount,
                    TotalRecords = result.DownloadedCount + result.UploadedCount,
                    ProcessedRecords = result.DownloadedCount + result.UploadedCount,
                    CurrentAction = "Sync completed.",
                    Complete = true,
                };

                if (result.ErrorSummary is not null)
                {
                    syncResponse.Error = result.ErrorSummary;
                }

                return syncResponse;
            }
#pragma warning disable CA1031 // Do not catch general exception types — sync must not crash the engine
            catch (Exception ex)
#pragma warning restore CA1031
            {
                return new SyncWithQrzResponse
                {
                    Complete = true,
                    Error = $"{ex.Message}\n{ex.StackTrace}",
                };
            }
        }
    }

    public GetSyncStatusResponse GetSyncStatus()
    {
        lock (_gate)
        {
            var counts = Sync(_storage.Logbook.GetCountsAsync());
            var syncMeta = Sync(_storage.Logbook.GetSyncMetadataAsync());

            var response = new GetSyncStatusResponse
            {
                LocalQsoCount = (uint)counts.LocalQsoCount,
                QrzQsoCount = _hasQrzLogbookApiKey ? (uint)(counts.LocalQsoCount - counts.PendingUploadCount) : 0,
                PendingUpload = (uint)counts.PendingUploadCount,
                IsSyncing = false,
                AutoSyncEnabled = _syncConfig.AutoSyncEnabled && _hasQrzLogbookApiKey,
            };

            if (syncMeta.LastSync is { } lastSyncUtc)
            {
                response.LastSync = Timestamp.FromDateTimeOffset(lastSyncUtc);
            }

            var active = GetEffectiveActiveProfileNoLock();
            if (_hasQrzLogbookApiKey && !string.IsNullOrWhiteSpace(active?.StationCallsign))
            {
                response.QrzLogbookOwner = active.StationCallsign;
            }

            return response;
        }
    }

    public ImportAdifResponse ImportAdif(byte[] adifBytes, bool refresh)
    {
        ArgumentNullException.ThrowIfNull(adifBytes);

        var qsos = ManagedAdifCodec.ParseAdiQsos(adifBytes);
        lock (_gate)
        {
            var response = new ImportAdifResponse();
            var activeStationProfile = GetEffectiveActiveProfileNoLock();
            var allExisting = Sync(_storage.Logbook.ListQsosAsync(new QsoListQuery())).ToList();

            for (var index = 0; index < qsos.Count; index++)
            {
                var recordNumber = index + 1;
                var qso = qsos[index].Clone();
                var hadImportedStationContext = ManagedQsoParity.QsoHasStationContext(qso);

                if (hadImportedStationContext)
                {
                    ManagedQsoParity.MaterializeStationSnapshotForCreate(qso, null);
                }
                else if (activeStationProfile is not null)
                {
                    ManagedQsoParity.MaterializeStationSnapshotForCreate(qso, activeStationProfile);
                    response.Warnings.Add(
                        $"Record {recordNumber}: local-station history was absent in ADIF; applied active station profile '{ManagedQsoParity.StationProfileLabel(activeStationProfile)}'.");
                }
                else
                {
                    response.RecordsSkipped++;
                    response.Warnings.Add(
                        $"Record {recordNumber}: local-station history was absent in ADIF and no active station profile is configured; skipped.");
                    continue;
                }

                ManagedQsoParity.NormalizeQsoForPersistence(qso);
                if (ManagedQsoParity.InvalidImportReason(qso) is { } reason)
                {
                    response.RecordsSkipped++;
                    response.Warnings.Add($"Record {recordNumber}: {reason} Skipped.");
                    continue;
                }

                var existingMatch = allExisting.FindIndex(existing => ManagedQsoParity.QsosMatchForDuplicate(existing, qso));
                if (existingMatch >= 0)
                {
                    if (refresh)
                    {
                        var merged = ManagedQsoParity.MergeQsoForRefresh(allExisting[existingMatch], qso);
                        Sync(_storage.Logbook.UpdateQsoAsync(merged));
                        allExisting[existingMatch] = merged;
                        response.RecordsUpdated++;
                        response.Warnings.Add($"Record {recordNumber}: refreshed existing record '{merged.LocalId}'.");
                    }
                    else
                    {
                        response.RecordsSkipped++;
                        response.Warnings.Add(
                            $"Record {recordNumber}: duplicate skipped; matched an existing QSO on station_callsign, worked_callsign, utc_timestamp, band, mode, and compatible submode/frequency.");
                    }

                    continue;
                }

                ValidateQsoNoLock(qso);
                FinalizeQsoForWrite(qso, isNew: true);
                qso.SyncStatus = SyncStatus.LocalOnly;
                Sync(_storage.Logbook.InsertQsoAsync(qso));
                allExisting.Add(qso);
                response.RecordsImported++;
            }

            return response;
        }
    }

    public byte[] ExportAdif(ExportAdifRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            var storageQuery = new QsoListQuery
            {
                After = request.After is not null ? request.After.ToDateTimeOffset() : null,
                Before = request.Before is not null ? request.Before.ToDateTimeOffset() : null,
                ContestId = NormalizeOptional(request.ContestId),
                Sort = Storage.QsoSortOrder.OldestFirst,
            };

            var qsos = Sync(_storage.Logbook.ListQsosAsync(storageQuery));
            return ManagedAdifCodec.SerializeAdiQsos(qsos, request.IncludeHeader);
        }
    }

    public LookupResponse Lookup(string callsign, bool cacheOnly = false, bool skipCache = false)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        if (cacheOnly)
        {
            var cached = _lookupCoordinator.GetCachedAsync(callsign).GetAwaiter().GetResult();
            return new LookupResponse { Result = cached };
        }

        var result = _lookupCoordinator.LookupAsync(callsign, skipCache).GetAwaiter().GetResult();

        // Also persist to the snapshot store for backward compat with existing storage code.
        var normalized = callsign.Trim().ToUpperInvariant();
        Sync(_storage.LookupSnapshots.UpsertAsync(new LookupSnapshot
        {
            Callsign = normalized,
            Result = result.Clone(),
            StoredAt = DateTimeOffset.UtcNow,
        }));

        return new LookupResponse { Result = result };
    }

    public StreamLookupResponse[] StreamLookup(string callsign)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        var results = _lookupCoordinator.StreamLookupAsync(callsign).GetAwaiter().GetResult();
        return results.Select(r => new StreamLookupResponse { Result = r }).ToArray();
    }

    public GetRigStatusResponse CreateRigStatusResponse()
    {
        if (_rigControlMonitor is null)
        {
            lock (_gate)
            {
                if (_rigControl is null || !_rigControl.Enabled)
                {
                    return new GetRigStatusResponse
                    {
                        Status = RigConnectionStatus.Disabled,
                        ErrorMessage = "Rig control is disabled in the managed engine.",
                    };
                }

                var response = new GetRigStatusResponse
                {
                    Status = RigConnectionStatus.Connected,
                };
                if (_rigControl.HasHost && _rigControl.HasPort)
                {
                    response.Endpoint = $"{_rigControl.Host}:{_rigControl.Port}";
                }

                return response;
            }
        }

        var snapshot = _rigControlMonitor.CurrentSnapshot();
        var result = new GetRigStatusResponse { Status = snapshot.Status };
        if (snapshot.HasErrorMessage)
        {
            result.ErrorMessage = snapshot.ErrorMessage;
        }

        return result;
    }

    public RigSnapshot BuildRigSnapshot()
    {
        if (_rigControlMonitor is not null)
        {
            return _rigControlMonitor.CurrentSnapshot();
        }

        var status = CreateRigStatusResponse();
        var snapshot = new RigSnapshot
        {
            FrequencyHz = status.Status == RigConnectionStatus.Connected ? 14_074_000UL : 0UL,
            Band = status.Status == RigConnectionStatus.Connected ? Band._20M : Band.Unspecified,
            Mode = status.Status == RigConnectionStatus.Connected ? Mode.Ft8 : Mode.Unspecified,
            Status = status.Status,
            SampledAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
        };

        if (status.HasErrorMessage)
        {
            snapshot.ErrorMessage = status.ErrorMessage;
        }

        return snapshot;
    }

    public TestRigConnectionResponse TestRigConnection()
    {
        if (_rigControlMonitor is not null)
        {
            var refreshed = _rigControlMonitor.RefreshSnapshot();
            var response = new TestRigConnectionResponse
            {
                Success = refreshed.Status == RigConnectionStatus.Connected,
            };

            if (refreshed.HasErrorMessage)
            {
                response.ErrorMessage = refreshed.ErrorMessage;
            }

            if (refreshed.Status == RigConnectionStatus.Connected)
            {
                response.Snapshot = refreshed;
            }

            return response;
        }

        var snapshot = BuildRigSnapshot();
        var fallbackResponse = new TestRigConnectionResponse
        {
            Success = snapshot.Status == RigConnectionStatus.Connected,
        };

        if (snapshot.Status == RigConnectionStatus.Connected)
        {
            fallbackResponse.Snapshot = snapshot;
        }
        else
        {
            fallbackResponse.ErrorMessage = "Rig control is disabled in the managed engine.";
        }

        return fallbackResponse;
    }

    public SpaceWeatherSnapshot BuildSpaceWeatherSnapshot(bool refreshed)
    {
        if (_spaceWeatherMonitor is null)
        {
            return new SpaceWeatherSnapshot
            {
                Status = SpaceWeatherStatus.Error,
                ErrorMessage = "Space weather not configured",
                SourceName = "NOAA SWPC",
            };
        }

        return refreshed
            ? _spaceWeatherMonitor.RefreshSnapshot()
            : _spaceWeatherMonitor.CurrentSnapshot();
    }

    public RuntimeConfigSnapshot GetRuntimeConfigSnapshot()
    {
        lock (_gate)
        {
            return BuildRuntimeConfigSnapshotNoLock();
        }
    }

    public RuntimeConfigSnapshot ApplyRuntimeConfig(IReadOnlyList<RuntimeConfigMutation> mutations)
    {
        ArgumentNullException.ThrowIfNull(mutations);

        lock (_gate)
        {
            foreach (var mutation in mutations)
            {
                switch (mutation.Key)
                {
                    case StorageBackendKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Set
                            && !string.Equals(mutation.Value, _storage.BackendName, StringComparison.OrdinalIgnoreCase))
                        {
                            throw new InvalidOperationException(
                                $"The managed .NET engine storage backend is fixed at startup. Restart the engine to use '{mutation.Value}'.");
                        }

                        _runtimeOverrides.Remove(StorageBackendKey);
                        break;
                    case QrzXmlUsernameKey:
                        ApplyStringOverrideNoLock(QrzXmlUsernameKey, mutation, value => _qrzXmlUsername = NormalizeOptional(value));
                        RebuildLookupCoordinatorNoLock();
                        break;
                    case QrzXmlPasswordKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
                        {
                            _qrzXmlPassword = null;
                            _hasQrzXmlPassword = false;
                            _runtimeOverrides[QrzXmlPasswordKey] = string.Empty;
                            RebuildLookupCoordinatorNoLock();
                        }
                        else
                        {
                            var newPassword = string.IsNullOrWhiteSpace(mutation.Value) ? null : mutation.Value.Trim();
                            if (newPassword is not null && newPassword != "***")
                            {
                                _qrzXmlPassword = newPassword;
                                RebuildLookupCoordinatorNoLock();
                            }

                            _hasQrzXmlPassword = _qrzXmlPassword is not null;
                            _runtimeOverrides[QrzXmlPasswordKey] = "***";
                        }

                        break;
                    case QrzLogbookApiKeyKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
                        {
                            _qrzLogbookApiKey = null;
                            _hasQrzLogbookApiKey = false;
                            _runtimeOverrides[QrzLogbookApiKeyKey] = string.Empty;
                        }
                        else
                        {
                            _qrzLogbookApiKey = string.IsNullOrWhiteSpace(mutation.Value) ? null : mutation.Value.Trim();
                            _hasQrzLogbookApiKey = _qrzLogbookApiKey is not null;
                            _runtimeOverrides[QrzLogbookApiKeyKey] = _hasQrzLogbookApiKey ? "***" : string.Empty;
                        }

                        if (_ownsSyncEngine)
                        {
                            RebuildSyncEngineNoLock();
                        }
                        break;
                    case RigEnabledKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
                        {
                            _rigControl ??= new RigControlSettings();
                            _rigControl.Enabled = false;
                            _runtimeOverrides[RigEnabledKey] = string.Empty;
                        }
                        else if (bool.TryParse(mutation.Value, out var enabled))
                        {
                            _rigControl ??= new RigControlSettings();
                            _rigControl.Enabled = enabled;
                            _runtimeOverrides[RigEnabledKey] = enabled ? "TRUE" : "FALSE";
                        }
                        else
                        {
                            throw new InvalidOperationException("QSORIPPER_RIGCTLD_ENABLED expects true or false.");
                        }

                        break;
                    default:
                        throw new InvalidOperationException($"Unsupported runtime config key: {mutation.Key}");
                }
            }

            return BuildRuntimeConfigSnapshotNoLock();
        }
    }

    public RuntimeConfigSnapshot ResetRuntimeConfig(IReadOnlyList<string> keys)
    {
        ArgumentNullException.ThrowIfNull(keys);

        lock (_gate)
        {
            var normalizedKeys = keys.Where(key => !string.IsNullOrWhiteSpace(key)).ToArray();
            if (normalizedKeys.Length == 0)
            {
                _runtimeOverrides.Clear();
                _qrzXmlUsername = NormalizeOptional(_persistedSetup.QrzXmlUsername);
                _qrzXmlPassword = NormalizeOptional(_persistedSetup.QrzXmlPassword);
                _hasQrzXmlPassword = _qrzXmlPassword is not null;
                _qrzLogbookApiKey = NormalizeOptional(_persistedSetup.QrzLogbookApiKey);
                _hasQrzLogbookApiKey = _qrzLogbookApiKey is not null;
                _rigControl = _persistedSetup.RigControl?.Clone();
                RebuildLookupCoordinatorNoLock();
                if (_ownsSyncEngine)
                {
                    RebuildSyncEngineNoLock();
                }
            }
            else
            {
                foreach (var key in normalizedKeys)
                {
                    switch (key)
                    {
                        case StorageBackendKey:
                            _runtimeOverrides.Remove(StorageBackendKey);
                            break;
                        case QrzXmlUsernameKey:
                            _qrzXmlUsername = NormalizeOptional(_persistedSetup.QrzXmlUsername);
                            _runtimeOverrides.Remove(QrzXmlUsernameKey);
                            RebuildLookupCoordinatorNoLock();
                            break;
                        case QrzXmlPasswordKey:
                            _qrzXmlPassword = NormalizeOptional(_persistedSetup.QrzXmlPassword);
                            _hasQrzXmlPassword = _qrzXmlPassword is not null;
                            _runtimeOverrides.Remove(QrzXmlPasswordKey);
                            RebuildLookupCoordinatorNoLock();
                            break;
                        case QrzLogbookApiKeyKey:
                            _qrzLogbookApiKey = NormalizeOptional(_persistedSetup.QrzLogbookApiKey);
                            _hasQrzLogbookApiKey = _qrzLogbookApiKey is not null;
                            _runtimeOverrides.Remove(QrzLogbookApiKeyKey);
                            if (_ownsSyncEngine)
                            {
                                RebuildSyncEngineNoLock();
                            }
                            break;
                        case RigEnabledKey:
                            _rigControl = _persistedSetup.RigControl?.Clone();
                            _runtimeOverrides.Remove(RigEnabledKey);
                            break;
                        default:
                            throw new InvalidOperationException($"Unsupported runtime config key: {key}");
                    }
                }
            }

            return BuildRuntimeConfigSnapshotNoLock();
        }
    }

    private void PersistNoLock()
    {
        SharedSetupConfigPersistence.Save(_configPath, _persistedSetup);
    }

    private SetupStatus BuildSetupStatusNoLock()
    {
        var isSqlite = IsSqliteBackendNoLock();
        var persistedProfile = GetPersistedActiveProfileNoLock() ?? new StationProfile();
        var persistedPath = NormalizeOptional(_persistedSetup.GetPersistedLogFilePath())
            ?? (isSqlite ? NormalizeOptional(_currentPersistenceLocation) : null);
        var status = new SetupStatus
        {
            ConfigFileExists = File.Exists(_configPath),
            SetupComplete = IsSetupCompleteNoLock(),
            ConfigPath = _configPath,
            HasStationProfile = _stationProfiles.Count > 0,
            StationProfile = persistedProfile,
            StationProfileCount = (uint)_stationProfiles.Count,
            IsFirstRun = !File.Exists(_configPath),
            HasQrzXmlPassword = !string.IsNullOrWhiteSpace(_persistedSetup.QrzXmlPassword),
            HasQrzLogbookApiKey = !string.IsNullOrWhiteSpace(_persistedSetup.QrzLogbookApiKey),
            PersistenceDescription = isSqlite ? PersistenceStepDescriptionSqlite : PersistenceStepDescription,
            PersistenceLabel = PersistenceStepLabel,
            PersistenceContractExplicit = true,
            SyncConfig = _persistedSetup.SyncConfig.Clone(),
        };
#pragma warning disable CS0612
        status.StorageBackend = isSqlite ? StorageBackend.Sqlite : StorageBackend.Memory;
#pragma warning restore CS0612
        status.PersistenceStepEnabled = isSqlite;

        if (!string.IsNullOrWhiteSpace(_persistedSetup.QrzXmlUsername))
        {
            status.QrzXmlUsername = _persistedSetup.QrzXmlUsername;
        }

        if (!string.IsNullOrWhiteSpace(_persistedSetup.ActiveProfileId))
        {
            status.ActiveStationProfileId = _persistedSetup.ActiveProfileId;
        }

        if (_persistedSetup.RigControl is not null)
        {
            status.RigControl = _persistedSetup.RigControl.Clone();
        }

        if (isSqlite)
        {
            status.PersistenceDefinitions.Add(
                new RuntimeConfigDefinition
                {
                    Key = PersistenceSetup.PathKey,
                    Label = PersistenceStepLabel,
                    Description = "SQLite logbook path for the shared durable setup.",
                    Kind = RuntimeConfigValueKind.Path,
                    Required = true,
                });

            if (!string.IsNullOrWhiteSpace(persistedPath))
            {
                status.LogFilePath = persistedPath;
                status.SuggestedLogFilePath = persistedPath;
                status.PersistenceValues.Add(
                    new RuntimeConfigValue
                    {
                        Key = PersistenceSetup.PathKey,
                        HasValue = true,
                        DisplayValue = persistedPath,
                    });
            }
        }

        if (!isSqlite)
        {
            status.Warnings.Add("Managed .NET engine currently uses an in-memory logbook.");
        }

        return status;
    }

    private SetupWizardStepStatus[] BuildStepStatusesNoLock()
    {
        return
        [
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.LogFile,
                Complete = !IsSqliteBackendNoLock() || !string.IsNullOrWhiteSpace(_persistedSetup.GetPersistedLogFilePath()) || !string.IsNullOrWhiteSpace(_currentPersistenceLocation),
            },
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.StationProfiles,
                Complete = GetPersistedActiveProfileNoLock() is { } active && IsStationProfileComplete(active),
            },
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.QrzIntegration,
                Complete = !string.IsNullOrWhiteSpace(_persistedSetup.QrzXmlUsername)
                    && !string.IsNullOrWhiteSpace(_persistedSetup.QrzXmlPassword),
            },
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.Review,
                Complete = IsSetupCompleteNoLock(),
            }
        ];
    }

    private List<StationProfileRecord> BuildStationProfileRecordsNoLock()
    {
        return _stationProfiles
            .Select(BuildStationProfileRecordNoLock)
            .ToList();
    }

    private StationProfileRecord BuildStationProfileRecordNoLock(ManagedPersistedStationProfile entry)
    {
        return new StationProfileRecord
        {
            ProfileId = entry.ProfileId,
            Profile = ParseProtoOrDefault<StationProfile>(entry.ProfileJson),
            IsActive = string.Equals(_activeProfileId, entry.ProfileId, StringComparison.Ordinal),
        };
    }

    private ActiveStationContext BuildActiveStationContextNoLock()
    {
        var persistedActive = GetPersistedActiveProfileNoLock() ?? new StationProfile();
        var effectiveActive = GetEffectiveActiveProfileNoLock() ?? new StationProfile();
        var context = new ActiveStationContext
        {
            PersistedActiveProfile = persistedActive.Clone(),
            EffectiveActiveProfile = effectiveActive.Clone(),
            HasSessionOverride = _sessionOverrideProfile is not null,
            SessionOverrideProfile = _sessionOverrideProfile?.Clone() ?? new StationProfile(),
        };

        if (!string.IsNullOrWhiteSpace(_activeProfileId))
        {
            context.PersistedActiveProfileId = _activeProfileId;
        }

        if (!IsSetupCompleteNoLock())
        {
            context.Warnings.Add("Managed engine setup is incomplete.");
        }

        return context;
    }

    private StationProfileRecord SaveStationProfileNoLock(string profileId, StationProfile profile, bool makeActive)
    {
        var normalizedId = NormalizeProfileIdOrDefault(profileId, profile.ProfileName, profile.StationCallsign);
        var serialized = ProtoJsonFormatter.Format(profile.Clone());
        var existing = _stationProfiles.FirstOrDefault(entry => string.Equals(entry.ProfileId, normalizedId, StringComparison.Ordinal));
        if (existing is null)
        {
            _stationProfiles.Add(new ManagedPersistedStationProfile
            {
                ProfileId = normalizedId,
                ProfileJson = serialized,
            });
        }
        else
        {
            existing.ProfileJson = serialized;
        }

        if (makeActive || string.IsNullOrWhiteSpace(_activeProfileId))
        {
            _activeProfileId = normalizedId;
        }

        return BuildStationProfileRecordNoLock(_stationProfiles.First(entry => string.Equals(entry.ProfileId, normalizedId, StringComparison.Ordinal)));
    }

    private StationProfile? GetPersistedActiveProfileNoLock()
    {
        var stored = _stationProfiles.FirstOrDefault(entry => string.Equals(entry.ProfileId, _activeProfileId, StringComparison.Ordinal));
        return stored is null ? null : ParseProtoOrDefault<StationProfile>(stored.ProfileJson);
    }

    private StationProfile? GetEffectiveActiveProfileNoLock()
    {
        return _sessionOverrideProfile?.Clone() ?? GetPersistedActiveProfileNoLock();
    }

    private void SyncPersistedProfilesNoLock()
    {
        _persistedSetup.ActiveProfileId = _activeProfileId;
        _persistedSetup.StationProfiles.Clear();

        foreach (var entry in _stationProfiles)
        {
            _persistedSetup.StationProfiles.Add(
                new ManagedPersistedStationProfile
                {
                    ProfileId = entry.ProfileId,
                    ProfileJson = entry.ProfileJson,
                });
        }
    }

    private void UpdatePersistedStorageSettingsNoLock(SaveSetupRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        var requestedPath = request.PersistenceValues
            .FirstOrDefault(static value => PersistenceSetup.IsPathKey(value.Key))
            ?.Value;

        requestedPath = NormalizeOptional(requestedPath)
            ?? NormalizeOptional(request.LogFilePath);

#pragma warning disable CS0612
        requestedPath ??= NormalizeOptional(request.SqlitePath);
#pragma warning restore CS0612

        if (IsSqliteBackendNoLock())
        {
            _persistedSetup.LogbookFilePath = requestedPath
                ?? NormalizeOptional(_currentPersistenceLocation)
                ?? NormalizeOptional(_persistedSetup.GetPersistedLogFilePath());
            _persistedSetup.StorageBackend = null;
            _persistedSetup.StorageSqlitePath = null;
            return;
        }

        _persistedSetup.LogbookFilePath = null;
        _persistedSetup.StorageBackend = "memory";
        _persistedSetup.StorageSqlitePath = null;
    }

    private bool IsSqliteBackendNoLock()
    {
        return string.Equals(_storage.BackendName, "sqlite", StringComparison.OrdinalIgnoreCase);
    }

    private void ApplyStationContextNoLock(QsoRecord qso, QsoRecord? existing = null)
    {
        if (existing is null)
        {
            ManagedQsoParity.MaterializeStationSnapshotForCreate(qso, GetEffectiveActiveProfileNoLock());
            return;
        }

        ManagedQsoParity.MaterializeStationSnapshotForUpdate(qso, existing);
    }

    private static void ValidateQsoNoLock(QsoRecord qso)
    {
        ManagedQsoParity.ValidateQsoForPersistence(qso);
    }

    private static void FinalizeQsoForWrite(QsoRecord qso, bool isNew)
    {
        if (string.IsNullOrWhiteSpace(qso.LocalId))
        {
            qso.LocalId = Guid.NewGuid().ToString();
        }

        if (IsTimestampUnset(qso.UtcTimestamp))
        {
            qso.UtcTimestamp = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow);
        }

        if (isNew && IsTimestampUnset(qso.CreatedAt))
        {
            qso.CreatedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow);
        }

        qso.UpdatedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow);
    }

    private void ApplySyncFlagsNoLock(QsoRecord qso, bool syncToQrz, LogQsoResponse response)
    {
        if (!syncToQrz)
        {
            qso.SyncStatus = SyncStatus.LocalOnly;
            response.SyncSuccess = false;
            response.SyncError = "Managed engine queued the QSO locally only.";
            return;
        }

        if (!_hasQrzLogbookApiKey)
        {
            qso.SyncStatus = SyncStatus.LocalOnly;
            response.SyncSuccess = false;
            response.SyncError = "QRZ logbook is not configured.";
            return;
        }

        qso.SyncStatus = SyncStatus.Synced;
        qso.QrzLogid = $"managed-{Guid.NewGuid():N}";
        response.QrzLogid = qso.QrzLogid;
        response.SyncSuccess = true;
    }

    private void ApplySyncFlagsNoLock(QsoRecord qso, bool syncToQrz, UpdateQsoResponse response)
    {
        if (!syncToQrz)
        {
            if (qso.SyncStatus == SyncStatus.Synced)
            {
                qso.SyncStatus = SyncStatus.Modified;
            }

            response.SyncSuccess = false;
            response.SyncError = "Managed engine updated the QSO locally only.";
            return;
        }

        if (!_hasQrzLogbookApiKey)
        {
            qso.SyncStatus = SyncStatus.Modified;
            response.SyncSuccess = false;
            response.SyncError = "QRZ logbook is not configured.";
            return;
        }

        qso.SyncStatus = SyncStatus.Synced;
        if (!qso.HasQrzLogid)
        {
            qso.QrzLogid = $"managed-{Guid.NewGuid():N}";
        }

        response.SyncSuccess = true;
    }

    private void RebuildLookupCoordinatorNoLock()
    {
        var username = Environment.GetEnvironmentVariable(QrzXmlUsernameKey)?.Trim()
            ?? _qrzXmlUsername;
        var password = Environment.GetEnvironmentVariable(QrzXmlPasswordKey)?.Trim()
            ?? _qrzXmlPassword;
        var userAgent = Environment.GetEnvironmentVariable(QrzUserAgentKey)?.Trim()
            ?? NormalizeOptional(_persistedSetup.QrzXmlUserAgent);

        if (!string.IsNullOrWhiteSpace(username) && !string.IsNullOrWhiteSpace(password))
        {
            // HttpClient is intentionally not disposed — it is a singleton owned by the provider for the app lifetime.
#pragma warning disable CA2000 // Dispose objects before losing scope
            var httpClient = new HttpClient { Timeout = TimeSpan.FromSeconds(8) };
#pragma warning restore CA2000
            _lookupCoordinator = new LookupCoordinator(
                new Lookup.Qrz.QrzXmlProvider(httpClient, username, password, userAgent: userAgent),
                _storage.LookupSnapshots);
        }
        else
        {
            _lookupCoordinator = new LookupCoordinator(
                new Lookup.Qrz.DisabledCallsignProvider(),
                _storage.LookupSnapshots);
        }
    }

    private void RebuildSyncEngineNoLock()
    {
        var apiKey = Environment.GetEnvironmentVariable(QrzLogbookApiKeyKey)?.Trim()
            ?? _qrzLogbookApiKey;
        if (string.IsNullOrWhiteSpace(apiKey))
        {
            DisposeOwnedSyncResourcesNoLock();
            return;
        }

        var baseUrl = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_LOGBOOK_BASE_URL")?.Trim()
            ?? NormalizeOptional(_persistedSetup.QrzLogbookBaseUrl);
        Uri? apiUri = null;
        if (!string.IsNullOrWhiteSpace(baseUrl))
        {
            apiUri = Uri.TryCreate(baseUrl, UriKind.Absolute, out var parsedUri)
                ? parsedUri
                : throw new InvalidOperationException($"Invalid QRZ logbook base URL '{baseUrl}'.");
        }

        var nextClient = apiUri is null
            ? new QrzLogbookClient(apiKey)
            : new QrzLogbookClient(apiKey, apiUri);
        var previousOwnedSyncClient = _ownedSyncClient;
        _ownedSyncClient = nextClient;
        _syncEngine = new QrzSyncEngine(nextClient);
        previousOwnedSyncClient?.Dispose();
    }

    private void DisposeOwnedSyncResourcesNoLock()
    {
        _ownedSyncClient?.Dispose();
        _ownedSyncClient = null;
        _syncEngine = null;
    }

    private static LookupCoordinator CreateDefaultCoordinator(IEngineStorage storage)
    {
        ICallsignProvider provider;
        var username = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_XML_USERNAME")?.Trim();
        var password = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_XML_PASSWORD")?.Trim();
        var userAgent = Environment.GetEnvironmentVariable(QrzUserAgentKey)?.Trim();

        if (!string.IsNullOrWhiteSpace(username) && !string.IsNullOrWhiteSpace(password))
        {
            // HttpClient is intentionally not disposed — it is a singleton owned by the provider for the app lifetime.
#pragma warning disable CA2000 // Dispose objects before losing scope
            var httpClient = new HttpClient { Timeout = TimeSpan.FromSeconds(8) };
#pragma warning restore CA2000
            provider = new Lookup.Qrz.QrzXmlProvider(httpClient, username, password, userAgent: userAgent);
        }
        else
        {
            provider = new Lookup.Qrz.DisabledCallsignProvider();
        }

        return new LookupCoordinator(provider, storage.LookupSnapshots);
    }

    private static List<RuntimeConfigDefinition> BuildRuntimeConfigDefinitionsNoLock()
    {
        return
        [
            new RuntimeConfigDefinition
            {
                Key = StorageBackendKey,
                Label = "Storage backend",
                Description = "Active storage backend for this engine process. Change it through startup configuration rather than runtime mutations.",
                Kind = RuntimeConfigValueKind.String,
                AllowedValues = { "memory", "sqlite" },
            },
            new RuntimeConfigDefinition
            {
                Key = QrzXmlUsernameKey,
                Label = "QRZ XML username",
                Description = "Managed-engine sample QRZ XML username.",
                Kind = RuntimeConfigValueKind.String,
            },
            new RuntimeConfigDefinition
            {
                Key = QrzXmlPasswordKey,
                Label = "QRZ XML password",
                Description = "Managed-engine sample QRZ XML password.",
                Kind = RuntimeConfigValueKind.String,
                Secret = true,
            },
            new RuntimeConfigDefinition
            {
                Key = QrzLogbookApiKeyKey,
                Label = "QRZ logbook API key",
                Description = "Managed-engine sample QRZ logbook API key.",
                Kind = RuntimeConfigValueKind.String,
                Secret = true,
            },
            new RuntimeConfigDefinition
            {
                Key = RigEnabledKey,
                Label = "Rig control enabled",
                Description = "Enable the managed sample rig-control status responses.",
                Kind = RuntimeConfigValueKind.Boolean,
                AllowedValues = { "true", "false" },
            }
        ];
    }

    private RuntimeConfigSnapshot BuildRuntimeConfigSnapshotNoLock()
    {
        var isSqlite = IsSqliteBackendNoLock();
        var snapshot = new RuntimeConfigSnapshot
        {
            ActiveStorageBackend = _storage.BackendName,
            LookupProviderSummary = ManagedLookupProviderSummary,
            PersistenceSummary = isSqlite ? SqlitePersistenceSummary : InMemoryPersistenceSummary,
        };

        if (GetEffectiveActiveProfileNoLock() is { } activeProfile)
        {
            snapshot.ActiveStationProfile = activeProfile.Clone();
        }

        if (isSqlite && !string.IsNullOrWhiteSpace(_currentPersistenceLocation))
        {
            snapshot.PersistenceLocation = _currentPersistenceLocation;
        }

        if (!isSqlite)
        {
            snapshot.Warnings.Add("Managed .NET engine currently uses an in-memory logbook.");
        }

        snapshot.Definitions.AddRange(BuildRuntimeConfigDefinitionsNoLock());
        snapshot.Values.AddRange(BuildRuntimeConfigValuesNoLock());
        return snapshot;
    }

    private List<RuntimeConfigValue> BuildRuntimeConfigValuesNoLock()
    {
        return
        [
            BuildRuntimeValue(StorageBackendKey, _storage.BackendName, overridden: _runtimeOverrides.ContainsKey(StorageBackendKey), secret: false, redacted: false),
            BuildRuntimeValue(QrzXmlUsernameKey, _qrzXmlUsername, overridden: _runtimeOverrides.ContainsKey(QrzXmlUsernameKey), secret: false, redacted: false),
            BuildRuntimeValue(QrzXmlPasswordKey, _hasQrzXmlPassword ? "***" : null, overridden: _runtimeOverrides.ContainsKey(QrzXmlPasswordKey), secret: true, redacted: _hasQrzXmlPassword),
            BuildRuntimeValue(QrzLogbookApiKeyKey, _hasQrzLogbookApiKey ? "***" : null, overridden: _runtimeOverrides.ContainsKey(QrzLogbookApiKeyKey), secret: true, redacted: _hasQrzLogbookApiKey),
            BuildRuntimeValue(RigEnabledKey, (_rigControl?.Enabled ?? false) ? "TRUE" : "FALSE", overridden: _runtimeOverrides.ContainsKey(RigEnabledKey), secret: false, redacted: false),
        ];
    }

    private static RuntimeConfigValue BuildRuntimeValue(string key, string? value, bool overridden, bool secret, bool redacted)
    {
        return new RuntimeConfigValue
        {
            Key = key,
            HasValue = !string.IsNullOrWhiteSpace(value),
            DisplayValue = value ?? string.Empty,
            Overridden = overridden,
            Secret = secret,
            Redacted = redacted,
        };
    }

    private void ApplyStringOverrideNoLock(string key, RuntimeConfigMutation mutation, Action<string?> setter)
    {
        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
        {
            setter(null);
            _runtimeOverrides[key] = string.Empty;
            return;
        }

        var value = NormalizeOptional(mutation.Value);
        setter(value);
        if (value is null)
        {
            _runtimeOverrides.Remove(key);
        }
        else
        {
            _runtimeOverrides[key] = value;
        }
    }

    private bool IsSetupCompleteNoLock()
    {
        var active = GetPersistedActiveProfileNoLock();
        return active is not null && IsStationProfileComplete(active);
    }

    private static void AddValidation(ValidateSetupStepResponse response, string field, bool valid, string message)
    {
        response.Fields.Add(new SetupFieldValidation
        {
            Field = field,
            Valid = valid,
            Message = valid ? string.Empty : message,
        });
    }

    private static bool IsStationProfileComplete(StationProfile profile)
    {
        return !string.IsNullOrWhiteSpace(profile.ProfileName)
            && !string.IsNullOrWhiteSpace(profile.StationCallsign)
            && !string.IsNullOrWhiteSpace(profile.OperatorCallsign)
            && !string.IsNullOrWhiteSpace(profile.Grid);
    }

    private static bool LooksRejected(string value)
    {
        return value.Contains("bad", StringComparison.OrdinalIgnoreCase)
            || value.Contains("fail", StringComparison.OrdinalIgnoreCase)
            || value.Contains("reject", StringComparison.OrdinalIgnoreCase);
    }

    private static string? NormalizeOptional(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value.Trim();
    }

    private static bool IsTimestampUnset(Timestamp? value)
    {
        return value is null || (value.Seconds == 0 && value.Nanos == 0);
    }

    private static T ParseProtoOrDefault<T>(string? json)
        where T : class, IMessage<T>, new()
    {
        return string.IsNullOrWhiteSpace(json) ? new T() : ProtoJsonParser.Parse<T>(json);
    }

    private static T? ParseOptionalProto<T>(string? json)
        where T : class, IMessage<T>, new()
    {
        return string.IsNullOrWhiteSpace(json) ? null : ProtoJsonParser.Parse<T>(json);
    }

    private static string NormalizeProfileIdOrDefault(params string?[] candidates)
    {
        foreach (var candidate in candidates)
        {
            if (string.IsNullOrWhiteSpace(candidate))
            {
                continue;
            }

#pragma warning disable CA1308 // Profile IDs intentionally match Rust's lowercase normalization.
            var normalized = Regex.Replace(candidate.Trim().ToLowerInvariant(), "[^a-z0-9]+", "-").Trim('-');
#pragma warning restore CA1308
            if (!string.IsNullOrWhiteSpace(normalized))
            {
                return normalized;
            }
        }

        return "default";
    }

    /// <summary>Synchronously extracts the result of a completed <see cref="ValueTask{T}"/>.</summary>
    private static T Sync<T>(ValueTask<T> task) => task.GetAwaiter().GetResult();

    /// <summary>Synchronously awaits a completed <see cref="ValueTask"/>.</summary>
    private static void Sync(ValueTask task) => task.GetAwaiter().GetResult();

    /// <summary>Synchronously extracts the result of a <see cref="Task{T}"/>.</summary>
    private static T Sync<T>(Task<T> task) => task.GetAwaiter().GetResult();
}
