using System.Text.Json;
using System.Text.Json.Serialization;
using System.Text.RegularExpressions;
using Google.Protobuf;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Engine.DotNet;

internal sealed class ManagedEngineState
{
    private const string PersistencePathKey = "persistence.path";
    private const string PersistenceStepDescription = "The managed .NET engine uses an in-memory logbook. No persisted log path is required during setup.";
    private const string PersistenceStepLabel = "Storage";
    private const string PersistenceSummary = "In-memory logbook";

    private const string StorageBackendKey = "QSORIPPER_STORAGE_BACKEND";
    private const string QrzXmlUsernameKey = "QSORIPPER_QRZ_XML_USERNAME";
    private const string QrzXmlPasswordKey = "QSORIPPER_QRZ_XML_PASSWORD";
    private const string QrzLogbookApiKeyKey = "QSORIPPER_QRZ_LOGBOOK_API_KEY";
    private const string RigEnabledKey = "QSORIPPER_RIGCTLD_ENABLED";

    private const string ManagedLookupProviderSummary = "Managed sample provider";
    private const string ManagedStorageBackend = "memory";

    private static readonly JsonSerializerOptions SerializerOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        WriteIndented = true,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private static readonly JsonFormatter ProtoJsonFormatter = new(JsonFormatter.Settings.Default.WithFormatDefaultValues(true));
    private static readonly JsonParser ProtoJsonParser = new(JsonParser.Settings.Default.WithIgnoreUnknownFields(true));

    private readonly Lock _gate = new();
    private readonly List<QsoRecord> _recentQsos = [];
    private readonly Dictionary<string, LookupResult> _lookupCache = new(StringComparer.OrdinalIgnoreCase);
    private readonly string _configPath;
    private readonly string _suggestedLogFilePath;

    private string _logFilePath;
    private string? _qrzXmlUsername;
    private bool _hasQrzXmlPassword;
    private bool _hasQrzLogbookApiKey;
    private SyncConfig _syncConfig;
    private RigControlSettings? _rigControl;
    private readonly List<ManagedPersistedStationProfile> _stationProfiles;
    private string? _activeProfileId;
    private StationProfile? _sessionOverrideProfile;
    private DateTimeOffset? _lastSyncUtc;
    private readonly Dictionary<string, string> _runtimeOverrides;

    public ManagedEngineState(string configPath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(configPath);

        _configPath = Path.GetFullPath(configPath.Trim());
        _suggestedLogFilePath = Path.Combine(Path.GetDirectoryName(_configPath) ?? AppContext.BaseDirectory, "qsoripper-managed.db");

        var persisted = LoadPersistedState(_configPath);
        _logFilePath = string.IsNullOrWhiteSpace(persisted.LogFilePath)
            ? _suggestedLogFilePath
            : persisted.LogFilePath;
        _qrzXmlUsername = NormalizeOptional(persisted.QrzXmlUsername);
        _hasQrzXmlPassword = persisted.HasQrzXmlPassword;
        _hasQrzLogbookApiKey = persisted.HasQrzLogbookApiKey;
        _syncConfig = ParseProtoOrDefault<SyncConfig>(persisted.SyncConfigJson);
        _rigControl = ParseOptionalProto<RigControlSettings>(persisted.RigControlJson);
        _stationProfiles = persisted.StationProfiles.ToList();
        _activeProfileId = NormalizeOptional(persisted.ActiveProfileId);
        _sessionOverrideProfile = ParseOptionalProto<StationProfile>(persisted.SessionOverrideProfileJson);
        _lastSyncUtc = persisted.LastSyncUtc;
        _runtimeOverrides = new Dictionary<string, string>(persisted.RuntimeOverrides, StringComparer.OrdinalIgnoreCase);
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
                "lookup",
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
                ValidateOptionalPersistencePath(response, request);
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
            var response = new TestQrzLogbookCredentialsResponse
            {
                Success = true,
                QsoCount = (uint)_recentQsos.Count,
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
            var requestedPersistencePath =
                NormalizeOptional(FindPersistenceValue(request.PersistenceValues))
                ?? NormalizeOptional(request.LogFilePath);
            if (!string.IsNullOrWhiteSpace(requestedPersistencePath))
            {
                _logFilePath = requestedPersistencePath;
            }

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
            }

            if (!string.IsNullOrWhiteSpace(request.QrzXmlPassword))
            {
                _hasQrzXmlPassword = true;
                _runtimeOverrides[QrzXmlPasswordKey] = "***";
            }

            if (!string.IsNullOrWhiteSpace(request.QrzLogbookApiKey))
            {
                _hasQrzLogbookApiKey = true;
                _runtimeOverrides[QrzLogbookApiKeyKey] = "***";
            }

            if (request.SyncConfig is not null)
            {
                _syncConfig = request.SyncConfig.Clone();
            }

            if (request.RigControl is not null)
            {
                _rigControl = request.RigControl.Clone();
            }

            _runtimeOverrides[StorageBackendKey] = ManagedStorageBackend;
            if (!string.IsNullOrWhiteSpace(_qrzXmlUsername))
            {
                _runtimeOverrides[QrzXmlUsernameKey] = _qrzXmlUsername;
            }

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
            PersistNoLock();
            return BuildActiveStationContextNoLock();
        }
    }

    public ActiveStationContext ClearSessionStationProfileOverride()
    {
        lock (_gate)
        {
            _sessionOverrideProfile = null;
            PersistNoLock();
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
            _recentQsos.Insert(0, qso);
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

            var index = _recentQsos.FindIndex(item => string.Equals(item.LocalId, qso.LocalId, StringComparison.Ordinal));
            if (index < 0)
            {
                return new UpdateQsoResponse { Success = false, Error = $"QSO '{qso.LocalId}' was not found." };
            }

            ApplyStationContextNoLock(qso, _recentQsos[index]);
            ManagedQsoParity.NormalizeQsoForPersistence(qso);
            ValidateQsoNoLock(qso);
            FinalizeQsoForWrite(qso, isNew: false);

            var response = new UpdateQsoResponse { Success = true };
            ApplySyncFlagsNoLock(qso, request.SyncToQrz, response);
            _recentQsos[index] = qso;
            return response;
        }
    }

    public bool DeleteQso(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_gate)
        {
            var removed = _recentQsos.RemoveAll(item => string.Equals(item.LocalId, localId.Trim(), StringComparison.Ordinal));
            return removed > 0;
        }
    }

    public QsoRecord? GetQso(string localId)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(localId);

        lock (_gate)
        {
            return _recentQsos
                .FirstOrDefault(item => string.Equals(item.LocalId, localId.Trim(), StringComparison.Ordinal))
                ?.Clone();
        }
    }

    public IReadOnlyList<QsoRecord> ListQsos(ListQsosRequest request)
    {
        ArgumentNullException.ThrowIfNull(request);

        lock (_gate)
        {
            IEnumerable<QsoRecord> query = _recentQsos.Select(item => item.Clone());

            if (request.After is not null)
            {
                var after = request.After.ToDateTimeOffset();
                query = query.Where(item => item.UtcTimestamp.ToDateTimeOffset() > after);
            }

            if (request.Before is not null)
            {
                var before = request.Before.ToDateTimeOffset();
                query = query.Where(item => item.UtcTimestamp.ToDateTimeOffset() < before);
            }

            if (!string.IsNullOrWhiteSpace(request.CallsignFilter))
            {
                query = query.Where(item =>
                    item.WorkedCallsign.Contains(request.CallsignFilter.Trim(), StringComparison.OrdinalIgnoreCase));
            }

            if (request.HasBandFilter)
            {
                query = query.Where(item => item.Band == request.BandFilter);
            }

            if (request.HasModeFilter)
            {
                query = query.Where(item => item.Mode == request.ModeFilter);
            }

            if (!string.IsNullOrWhiteSpace(request.ContestId))
            {
                query = query.Where(item => string.Equals(item.ContestId, request.ContestId, StringComparison.OrdinalIgnoreCase));
            }

            query = request.Sort == QsoSortOrder.OldestFirst
                ? query.OrderBy(item => item.UtcTimestamp.ToDateTimeOffset()).ThenBy(item => item.LocalId, StringComparer.Ordinal)
                : query.OrderByDescending(item => item.UtcTimestamp.ToDateTimeOffset()).ThenByDescending(item => item.LocalId, StringComparer.Ordinal);

            if (request.Offset > 0)
            {
                query = query.Skip((int)request.Offset);
            }

            if (request.Limit > 0)
            {
                query = query.Take((int)request.Limit);
            }

            return query.ToArray();
        }
    }

    public SyncWithQrzResponse SyncWithQrz()
    {
        lock (_gate)
        {
            if (!_hasQrzLogbookApiKey)
            {
                return new SyncWithQrzResponse
                {
                    Complete = true,
                    Error = "QRZ logbook is not configured.",
                };
            }

            _lastSyncUtc = DateTimeOffset.UtcNow;
            var uploadedCount = 0u;

            for (var i = 0; i < _recentQsos.Count; i++)
            {
                if (_recentQsos[i].SyncStatus != SyncStatus.Synced)
                {
                    var updated = _recentQsos[i].Clone();
                    updated.SyncStatus = SyncStatus.Synced;
                    if (!updated.HasQrzLogid)
                    {
                        updated.QrzLogid = $"managed-{i + 1}";
                    }

                    _recentQsos[i] = updated;
                    uploadedCount++;
                }
            }

            return new SyncWithQrzResponse
            {
                TotalRecords = (uint)_recentQsos.Count,
                ProcessedRecords = (uint)_recentQsos.Count,
                UploadedRecords = uploadedCount,
                DownloadedRecords = 0,
                ConflictRecords = 0,
                CurrentAction = "Managed engine sync completed.",
                Complete = true,
            };
        }
    }

    public GetSyncStatusResponse GetSyncStatus()
    {
        lock (_gate)
        {
            var response = new GetSyncStatusResponse
            {
                LocalQsoCount = (uint)_recentQsos.Count,
                QrzQsoCount = _hasQrzLogbookApiKey ? (uint)_recentQsos.Count(item => item.SyncStatus == SyncStatus.Synced) : 0,
                PendingUpload = (uint)_recentQsos.Count(item => item.SyncStatus != SyncStatus.Synced),
                IsSyncing = false,
                AutoSyncEnabled = _syncConfig.AutoSyncEnabled && _hasQrzLogbookApiKey,
            };

            if (_lastSyncUtc is { } lastSyncUtc)
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

                var existingIndex = _recentQsos.FindIndex(existing => ManagedQsoParity.QsosMatchForDuplicate(existing, qso));
                if (existingIndex >= 0)
                {
                    if (refresh)
                    {
                        var merged = ManagedQsoParity.MergeQsoForRefresh(_recentQsos[existingIndex], qso);
                        _recentQsos[existingIndex] = merged;
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
                _recentQsos.Insert(0, qso);
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
            IEnumerable<QsoRecord> query = _recentQsos.Select(item => item.Clone());

            if (request.After is not null)
            {
                var after = request.After.ToDateTimeOffset();
                query = query.Where(item => item.UtcTimestamp is not null && item.UtcTimestamp.ToDateTimeOffset() > after);
            }

            if (request.Before is not null)
            {
                var before = request.Before.ToDateTimeOffset();
                query = query.Where(item => item.UtcTimestamp is not null && item.UtcTimestamp.ToDateTimeOffset() < before);
            }

            if (!string.IsNullOrWhiteSpace(request.ContestId))
            {
                query = query.Where(item => string.Equals(item.ContestId, request.ContestId, StringComparison.OrdinalIgnoreCase));
            }

            query = query
                .OrderBy(item => item.UtcTimestamp?.ToDateTimeOffset() ?? DateTimeOffset.MinValue)
                .ThenBy(item => item.LocalId, StringComparer.Ordinal);

            return ManagedAdifCodec.SerializeAdiQsos(query, request.IncludeHeader);
        }
    }

    public LookupResponse Lookup(string callsign, bool cacheOnly = false, bool skipCache = false)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        lock (_gate)
        {
            var normalized = callsign.Trim().ToUpperInvariant();
            if (!skipCache && _lookupCache.TryGetValue(normalized, out var cached))
            {
                var result = cached.Clone();
                result.CacheHit = true;
                result.LookupLatencyMs = 0;
                return new LookupResponse { Result = result };
            }

            if (cacheOnly)
            {
                return new LookupResponse
                {
                    Result = new LookupResult
                    {
                        State = LookupState.NotFound,
                        CacheHit = true,
                        LookupLatencyMs = 0,
                        QueriedCallsign = normalized,
                    }
                };
            }

            var resultToCache = BuildLookupResultNoLock(normalized);
            _lookupCache[normalized] = resultToCache.Clone();
            return new LookupResponse { Result = resultToCache };
        }
    }

    public StreamLookupResponse[] StreamLookup(string callsign)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(callsign);

        var normalized = callsign.Trim().ToUpperInvariant();
        return
        [
            new StreamLookupResponse
            {
                Result = new LookupResult
                {
                    State = LookupState.Loading,
                    CacheHit = false,
                    LookupLatencyMs = 0,
                    QueriedCallsign = normalized,
                }
            },
            new StreamLookupResponse
            {
                Result = Lookup(callsign).Result
            }
        ];
    }

    public GetRigStatusResponse CreateRigStatusResponse()
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

    public RigSnapshot BuildRigSnapshot()
    {
        var status = CreateRigStatusResponse();
        return new RigSnapshot
        {
            FrequencyHz = status.Status == RigConnectionStatus.Connected ? 14_074_000UL : 0UL,
            Band = status.Status == RigConnectionStatus.Connected ? Band._20M : Band.Unspecified,
            Mode = status.Status == RigConnectionStatus.Connected ? Mode.Ft8 : Mode.Unspecified,
            Status = status.Status,
            ErrorMessage = status.HasErrorMessage ? status.ErrorMessage : null,
            SampledAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow),
        };
    }

    public TestRigConnectionResponse TestRigConnection()
    {
        var snapshot = BuildRigSnapshot();
        return new TestRigConnectionResponse
        {
            Success = snapshot.Status == RigConnectionStatus.Connected,
            ErrorMessage = snapshot.Status == RigConnectionStatus.Connected ? null : "Rig control is disabled in the managed engine.",
            Snapshot = snapshot.Status == RigConnectionStatus.Connected ? snapshot : null,
        };
    }

    public static SpaceWeatherSnapshot BuildSpaceWeatherSnapshot(bool refreshed)
    {
        var now = DateTimeOffset.UtcNow;
        return new SpaceWeatherSnapshot
        {
            ObservedAt = Timestamp.FromDateTimeOffset(now),
            FetchedAt = Timestamp.FromDateTimeOffset(now),
            Status = SpaceWeatherStatus.Current,
            PlanetaryKIndex = refreshed ? 2.3 : 2.0,
            PlanetaryAIndex = 9,
            SolarFluxIndex = refreshed ? 152.0 : 148.0,
            SunspotNumber = 96,
            GeomagneticStormScale = 1,
            SourceName = "Managed sample provider",
        };
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
                            && !string.Equals(mutation.Value, ManagedStorageBackend, StringComparison.OrdinalIgnoreCase))
                        {
                            throw new InvalidOperationException("The managed .NET engine currently supports only memory storage.");
                        }

                        _runtimeOverrides[StorageBackendKey] = ManagedStorageBackend;
                        break;
                    case QrzXmlUsernameKey:
                        ApplyStringOverrideNoLock(QrzXmlUsernameKey, mutation, value => _qrzXmlUsername = NormalizeOptional(value));
                        break;
                    case QrzXmlPasswordKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
                        {
                            _hasQrzXmlPassword = false;
                            _runtimeOverrides.Remove(QrzXmlPasswordKey);
                        }
                        else
                        {
                            _hasQrzXmlPassword = !string.IsNullOrWhiteSpace(mutation.Value);
                            _runtimeOverrides[QrzXmlPasswordKey] = "***";
                        }

                        break;
                    case QrzLogbookApiKeyKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
                        {
                            _hasQrzLogbookApiKey = false;
                            _runtimeOverrides.Remove(QrzLogbookApiKeyKey);
                        }
                        else
                        {
                            _hasQrzLogbookApiKey = !string.IsNullOrWhiteSpace(mutation.Value);
                            _runtimeOverrides[QrzLogbookApiKeyKey] = "***";
                        }

                        break;
                    case RigEnabledKey:
                        if (mutation.Kind == RuntimeConfigMutationKind.Clear)
                        {
                            _rigControl ??= new RigControlSettings();
                            _rigControl.Enabled = false;
                            _runtimeOverrides.Remove(RigEnabledKey);
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

            PersistNoLock();
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
                _qrzXmlUsername = null;
                _hasQrzXmlPassword = false;
                _hasQrzLogbookApiKey = false;
                if (_rigControl is not null)
                {
                    _rigControl.Enabled = false;
                }
            }
            else
            {
                foreach (var key in normalizedKeys)
                {
                    switch (key)
                    {
                        case StorageBackendKey:
                            _runtimeOverrides[StorageBackendKey] = ManagedStorageBackend;
                            break;
                        case QrzXmlUsernameKey:
                            _qrzXmlUsername = null;
                            _runtimeOverrides.Remove(QrzXmlUsernameKey);
                            break;
                        case QrzXmlPasswordKey:
                            _hasQrzXmlPassword = false;
                            _runtimeOverrides.Remove(QrzXmlPasswordKey);
                            break;
                        case QrzLogbookApiKeyKey:
                            _hasQrzLogbookApiKey = false;
                            _runtimeOverrides.Remove(QrzLogbookApiKeyKey);
                            break;
                        case RigEnabledKey:
                            _rigControl ??= new RigControlSettings();
                            _rigControl.Enabled = false;
                            _runtimeOverrides.Remove(RigEnabledKey);
                            break;
                        default:
                            throw new InvalidOperationException($"Unsupported runtime config key: {key}");
                    }
                }
            }

            PersistNoLock();
            return BuildRuntimeConfigSnapshotNoLock();
        }
    }

    private static ManagedEnginePersistedState LoadPersistedState(string configPath)
    {
        if (!File.Exists(configPath))
        {
            return new ManagedEnginePersistedState
            {
                SyncConfigJson = ProtoJsonFormatter.Format(new SyncConfig
                {
                    AutoSyncEnabled = false,
                    SyncIntervalSeconds = 300,
                    ConflictPolicy = ConflictPolicy.LastWriteWins,
                })
            };
        }

        var json = File.ReadAllText(configPath);
        return JsonSerializer.Deserialize<ManagedEnginePersistedState>(json, SerializerOptions)
            ?? new ManagedEnginePersistedState();
    }

    private void PersistNoLock()
    {
        var directory = Path.GetDirectoryName(_configPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
        }

        var persisted = new ManagedEnginePersistedState
        {
            LogFilePath = _logFilePath,
            QrzXmlUsername = _qrzXmlUsername,
            HasQrzXmlPassword = _hasQrzXmlPassword,
            HasQrzLogbookApiKey = _hasQrzLogbookApiKey,
            SyncConfigJson = ProtoJsonFormatter.Format(_syncConfig),
            RigControlJson = _rigControl is null ? null : ProtoJsonFormatter.Format(_rigControl),
            ActiveProfileId = _activeProfileId,
            SessionOverrideProfileJson = _sessionOverrideProfile is null ? null : ProtoJsonFormatter.Format(_sessionOverrideProfile),
            LastSyncUtc = _lastSyncUtc,
        };

        foreach (var entry in _stationProfiles)
        {
            persisted.StationProfiles.Add(new ManagedPersistedStationProfile
            {
                ProfileId = entry.ProfileId,
                ProfileJson = entry.ProfileJson,
            });
        }

        foreach (var entry in _runtimeOverrides)
        {
            persisted.RuntimeOverrides[entry.Key] = entry.Value;
        }

        File.WriteAllText(_configPath, JsonSerializer.Serialize(persisted, SerializerOptions));
    }

    private SetupStatus BuildSetupStatusNoLock()
    {
        var status = new SetupStatus
        {
            ConfigFileExists = File.Exists(_configPath),
            SetupComplete = IsSetupCompleteNoLock(),
            ConfigPath = _configPath,
            HasStationProfile = _stationProfiles.Count > 0,
            StationProfile = GetEffectiveActiveProfileNoLock() ?? new StationProfile(),
            StationProfileCount = (uint)_stationProfiles.Count,
            SuggestedLogFilePath = _suggestedLogFilePath,
            IsFirstRun = !File.Exists(_configPath),
            HasQrzXmlPassword = _hasQrzXmlPassword,
            HasQrzLogbookApiKey = _hasQrzLogbookApiKey,
            PersistenceDescription = PersistenceStepDescription,
            PersistenceLabel = PersistenceStepLabel,
            PersistenceContractExplicit = true,
            SyncConfig = _syncConfig.Clone(),
        };
#pragma warning disable CS0612
        status.StorageBackend = StorageBackend.Memory;
#pragma warning restore CS0612
        status.PersistenceStepEnabled = false;

        if (!string.IsNullOrWhiteSpace(_logFilePath))
        {
            status.LogFilePath = _logFilePath;
        }

        if (!string.IsNullOrWhiteSpace(_qrzXmlUsername))
        {
            status.QrzXmlUsername = _qrzXmlUsername;
        }

        if (!string.IsNullOrWhiteSpace(_activeProfileId))
        {
            status.ActiveStationProfileId = _activeProfileId;
        }

        if (_rigControl is not null)
        {
            status.RigControl = _rigControl.Clone();
        }

        status.Warnings.Add("Managed .NET engine currently uses an in-memory logbook.");
        return status;
    }

    private SetupWizardStepStatus[] BuildStepStatusesNoLock()
    {
        return
        [
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.LogFile,
                Complete = true,
            },
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.StationProfiles,
                Complete = GetEffectiveActiveProfileNoLock() is { } active && IsStationProfileComplete(active),
            },
            new SetupWizardStepStatus
            {
                Step = SetupWizardStep.QrzIntegration,
                Complete = !string.IsNullOrWhiteSpace(_qrzXmlUsername) && _hasQrzXmlPassword,
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

    private static LookupResult BuildLookupResultNoLock(string normalizedCallsign)
    {
        if (normalizedCallsign.Contains("NOTFOUND", StringComparison.Ordinal))
        {
            return new LookupResult
            {
                State = LookupState.NotFound,
                CacheHit = false,
                LookupLatencyMs = 6,
                QueriedCallsign = normalizedCallsign,
            };
        }

        if (normalizedCallsign.Contains("ERROR", StringComparison.Ordinal))
        {
            return new LookupResult
            {
                State = LookupState.Error,
                ErrorMessage = "Managed sample lookup forced an error for this callsign.",
                CacheHit = false,
                LookupLatencyMs = 6,
                QueriedCallsign = normalizedCallsign,
            };
        }

        var record = new CallsignRecord
        {
            Callsign = normalizedCallsign,
            CrossRef = normalizedCallsign,
            FirstName = "Managed",
            LastName = "Operator",
            Nickname = "Managed",
            FormattedName = $"Managed Operator ({normalizedCallsign})",
            LicenseClass = "Reference",
            Addr1 = "1 Engine Way",
            Addr2 = "Redmond",
            State = "WA",
            Zip = "98052",
            Country = "United States",
            GridSquare = "CN87",
            County = "King",
            Latitude = 47.6740,
            Longitude = -122.1215,
            Email = "managed-engine@example.com",
            WebUrl = $"https://example.invalid/{normalizedCallsign}",
            CqZone = 3,
            ItuZone = 2,
            DxccCountryName = "United States",
            DxccContinent = "NA",
            Eqsl = QslPreference.Yes,
            Lotw = QslPreference.Yes,
            PaperQsl = QslPreference.No,
            TimeZone = "America/Los_Angeles",
        };
        record.Aliases.Add($"ALT-{normalizedCallsign}");

        return new LookupResult
        {
            State = LookupState.Found,
            Record = record,
            CacheHit = false,
            LookupLatencyMs = 6,
            QueriedCallsign = normalizedCallsign,
        };
    }

    private static List<RuntimeConfigDefinition> BuildRuntimeConfigDefinitionsNoLock()
    {
        return
        [
            new RuntimeConfigDefinition
            {
                Key = StorageBackendKey,
                Label = "Storage backend",
                Description = "Managed engine storage backend. The current implementation supports only memory.",
                Kind = RuntimeConfigValueKind.String,
                AllowedValues = { ManagedStorageBackend },
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
        var snapshot = new RuntimeConfigSnapshot
        {
            ActiveStorageBackend = ManagedStorageBackend,
            LookupProviderSummary = ManagedLookupProviderSummary,
            PersistenceSummary = PersistenceSummary,
        };

        if (GetEffectiveActiveProfileNoLock() is { } activeProfile)
        {
            snapshot.ActiveStationProfile = activeProfile.Clone();
        }

        snapshot.Warnings.Add("Managed .NET engine currently uses an in-memory logbook.");
        snapshot.Definitions.AddRange(BuildRuntimeConfigDefinitionsNoLock());
        snapshot.Values.AddRange(BuildRuntimeConfigValuesNoLock());
        return snapshot;
    }

    private List<RuntimeConfigValue> BuildRuntimeConfigValuesNoLock()
    {
        return
        [
            BuildRuntimeValue(StorageBackendKey, ManagedStorageBackend, overridden: true, secret: false, redacted: false),
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
            _runtimeOverrides.Remove(key);
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
        var active = GetEffectiveActiveProfileNoLock();
        return active is not null && IsStationProfileComplete(active);
    }

    private static string? FindPersistenceValue(IEnumerable<SetupFieldValue> values)
    {
        return values
            .FirstOrDefault(value => string.Equals(value.Key, PersistencePathKey, StringComparison.OrdinalIgnoreCase))
            ?.Value;
    }

    private static void ValidateOptionalPersistencePath(
        ValidateSetupStepResponse response,
        ValidateSetupStepRequest request)
    {
        var path = NormalizeOptional(FindPersistenceValue(request.PersistenceValues))
            ?? NormalizeOptional(request.LogFilePath);
        if (string.IsNullOrWhiteSpace(path))
        {
            response.Valid = true;
            return;
        }

        var parent = Path.GetDirectoryName(path);
        var valid = string.IsNullOrWhiteSpace(parent) || Directory.Exists(parent);
        AddValidation(
            response,
            PersistencePathKey,
            valid,
            valid ? string.Empty : $"Parent directory '{parent}' does not exist.");
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

            var normalized = Regex.Replace(candidate.Trim().ToUpperInvariant(), "[^A-Z0-9]+", "-").Trim('-');
            if (!string.IsNullOrWhiteSpace(normalized))
            {
                return normalized;
            }
        }

        return "default";
    }
}
