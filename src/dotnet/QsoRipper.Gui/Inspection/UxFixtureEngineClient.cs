using System;
using System.Collections.Generic;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Services;

namespace QsoRipper.Gui.Inspection;

internal sealed class UxFixtureEngineClient : IEngineClient
{
    private readonly object _gate = new();
    private readonly List<QsoRecord> _recentQsos;
    private StationProfile _stationProfile;
    private SyncConfig _syncConfig;
    private string _configPath;
    private string _logFilePath;
    private string? _qrzXmlUsername;
    private DateTimeOffset? _lastSyncUtc;
    private bool _configFileExists;
    private bool _setupComplete;
    private bool _isFirstRun;
    private bool _hasQrzXmlPassword;
    private bool _hasQrzLogbookApiKey;
    private bool _isSyncing;

    public UxFixtureEngineClient(UxCaptureFixture fixture)
    {
        _recentQsos = fixture.BuildRecentQsoRecords()
            .Select(record => record.Clone())
            .ToList();
        _stationProfile = fixture.BuildStationProfile();
        _syncConfig = fixture.BuildSyncConfig();
        _configPath = fixture.ConfigPath;
        _logFilePath = fixture.ActiveLogFilePath;
        _qrzXmlUsername = fixture.QrzXmlUsername;
        _lastSyncUtc = fixture.LastSyncUtc;
        _configFileExists = fixture.ConfigFileExists;
        _setupComplete = fixture.SetupComplete;
        _isFirstRun = fixture.IsFirstRun;
        _hasQrzXmlPassword = fixture.HasQrzXmlPassword;
        _hasQrzLogbookApiKey = fixture.HasQrzLogbookApiKey;
        _isSyncing = fixture.IsSyncing;
    }

    public Task<GetSetupWizardStateResponse> GetWizardStateAsync(CancellationToken ct = default)
    {
        lock (_gate)
        {
            var response = new GetSetupWizardStateResponse
            {
                Status = BuildSetupStatus()
            };

            response.Steps.AddRange(BuildStepStatuses());

            if (HasStationProfile())
            {
                response.StationProfiles.Add(
                    new StationProfileRecord
                    {
                        ProfileId = "capture-profile",
                        Profile = _stationProfile.Clone(),
                        IsActive = true
                    });
            }

            return Task.FromResult(response);
        }
    }

    public Task<ValidateSetupStepResponse> ValidateStepAsync(ValidateSetupStepRequest request, CancellationToken ct = default)
    {
        var response = new ValidateSetupStepResponse();

        switch (request.Step)
        {
            case SetupWizardStep.LogFile:
                AddValidation(
                    response,
                    "log_file_path",
                    !string.IsNullOrWhiteSpace(request.LogFilePath),
                    "Log file path is required.");
                break;

            case SetupWizardStep.StationProfiles:
                var profile = request.StationProfile ?? new StationProfile();
                AddValidation(
                    response,
                    "profile_name",
                    !string.IsNullOrWhiteSpace(profile.ProfileName),
                    "Profile name is required.");
                AddValidation(
                    response,
                    "callsign",
                    !string.IsNullOrWhiteSpace(profile.StationCallsign),
                    "Station callsign is required.");
                AddValidation(
                    response,
                    "operator_callsign",
                    !string.IsNullOrWhiteSpace(profile.OperatorCallsign),
                    "Operator callsign is required.");
                AddValidation(
                    response,
                    "grid_square",
                    !string.IsNullOrWhiteSpace(profile.Grid),
                    "Grid square is required.");
                break;

            case SetupWizardStep.QrzIntegration:
                var hasUsername = !string.IsNullOrWhiteSpace(request.QrzXmlUsername);
                var hasPassword = !string.IsNullOrWhiteSpace(request.QrzXmlPassword);
                AddValidation(
                    response,
                    "qrz_xml_username",
                    hasUsername == hasPassword,
                    "Provide both username and password, or leave both blank.");
                AddValidation(
                    response,
                    "qrz_xml_password",
                    hasUsername == hasPassword,
                    "Provide both username and password, or leave both blank.");
                break;
        }

        response.Valid = response.Fields.All(field => field.Valid);
        return Task.FromResult(response);
    }

    public Task<TestQrzCredentialsResponse> TestQrzCredentialsAsync(
        string username,
        string password,
        CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(username) || string.IsNullOrWhiteSpace(password))
        {
            return Task.FromResult(
                new TestQrzCredentialsResponse
                {
                    Success = false,
                    ErrorMessage = "Username and password are required."
                });
        }

        if (LooksRejected(username) || LooksRejected(password))
        {
            return Task.FromResult(
                new TestQrzCredentialsResponse
                {
                    Success = false,
                    ErrorMessage = "Fixture rejected the supplied QRZ XML credentials."
                });
        }

        return Task.FromResult(new TestQrzCredentialsResponse { Success = true });
    }

    public Task<SaveSetupResponse> SaveSetupAsync(SaveSetupRequest request, CancellationToken ct = default)
    {
        lock (_gate)
        {
            if (!string.IsNullOrWhiteSpace(request.LogFilePath))
            {
                _logFilePath = request.LogFilePath;
            }

            if (request.StationProfile is not null)
            {
                _stationProfile = request.StationProfile.Clone();
            }

            if (!string.IsNullOrWhiteSpace(request.QrzXmlUsername))
            {
                _qrzXmlUsername = request.QrzXmlUsername;
                _hasQrzXmlPassword = !string.IsNullOrWhiteSpace(request.QrzXmlPassword) || _hasQrzXmlPassword;
            }

            if (!string.IsNullOrWhiteSpace(request.QrzLogbookApiKey))
            {
                _hasQrzLogbookApiKey = true;
            }

            if (request.SyncConfig is not null)
            {
                _syncConfig = request.SyncConfig.Clone();
            }

            _configFileExists = true;
            _isFirstRun = false;
            _setupComplete = !string.IsNullOrWhiteSpace(_logFilePath) && IsStationProfileComplete(_stationProfile);

            return Task.FromResult(
                new SaveSetupResponse
                {
                    Status = BuildSetupStatus()
                });
        }
    }

    public Task<GetSetupStatusResponse> GetSetupStatusAsync(CancellationToken ct = default)
    {
        lock (_gate)
        {
            return Task.FromResult(
                new GetSetupStatusResponse
                {
                    Status = BuildSetupStatus()
                });
        }
    }

    public Task<TestQrzLogbookCredentialsResponse> TestQrzLogbookCredentialsAsync(string apiKey, CancellationToken ct = default)
    {
        if (string.IsNullOrWhiteSpace(apiKey))
        {
            return Task.FromResult(
                new TestQrzLogbookCredentialsResponse
                {
                    Success = false,
                    ErrorMessage = "API key is required."
                });
        }

        if (LooksRejected(apiKey))
        {
            return Task.FromResult(
                new TestQrzLogbookCredentialsResponse
                {
                    Success = false,
                    ErrorMessage = "Fixture rejected the supplied QRZ logbook API key."
                });
        }

        return Task.FromResult(
            new TestQrzLogbookCredentialsResponse
            {
                Success = true,
                QsoCount = (uint)_recentQsos.Count,
                LogbookOwner = _stationProfile.StationCallsign
            });
    }

    public Task<IReadOnlyList<QsoRecord>> ListRecentQsosAsync(int limit = 200, CancellationToken ct = default)
    {
        lock (_gate)
        {
            var records = _recentQsos
                .Take(limit)
                .Select(record => record.Clone())
                .ToArray();
            return Task.FromResult((IReadOnlyList<QsoRecord>)records);
        }
    }

    public Task<UpdateQsoResponse> UpdateQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default)
    {
        lock (_gate)
        {
            var existingIndex = _recentQsos.FindIndex(item => string.Equals(item.LocalId, qso.LocalId, StringComparison.Ordinal));
            if (existingIndex >= 0)
            {
                _recentQsos[existingIndex] = qso.Clone();
            }
            else
            {
                _recentQsos.Insert(0, qso.Clone());
            }

            return Task.FromResult(new UpdateQsoResponse { Success = true });
        }
    }

    public Task<SyncWithQrzResponse> SyncWithQrzAsync(CancellationToken ct = default)
    {
        lock (_gate)
        {
            if (!_hasQrzLogbookApiKey)
            {
                return Task.FromResult(
                    new SyncWithQrzResponse
                    {
                        Error = "QRZ logbook is not configured in the fixture."
                    });
            }

            _isSyncing = true;
            var uploadedCount = 0u;

            for (var i = 0; i < _recentQsos.Count; i++)
            {
                if (_recentQsos[i].SyncStatus != SyncStatus.Synced)
                {
                    var updated = _recentQsos[i].Clone();
                    updated.SyncStatus = SyncStatus.Synced;
                    _recentQsos[i] = updated;
                    uploadedCount++;
                }
            }

            _lastSyncUtc = DateTimeOffset.UtcNow;
            _isSyncing = false;

            return Task.FromResult(
                new SyncWithQrzResponse
                {
                    UploadedRecords = uploadedCount,
                    DownloadedRecords = 0
                });
        }
    }

    public Task<GetSyncStatusResponse> GetSyncStatusAsync(CancellationToken ct = default)
    {
        lock (_gate)
        {
            var response = new GetSyncStatusResponse
            {
                LocalQsoCount = (uint)_recentQsos.Count,
                QrzQsoCount = (uint)_recentQsos.Count,
                PendingUpload = (uint)_recentQsos.Count(record => record.SyncStatus != SyncStatus.Synced),
                AutoSyncEnabled = _syncConfig.AutoSyncEnabled,
                IsSyncing = _isSyncing
            };

            if (_lastSyncUtc is { } lastSyncUtc)
            {
                response.LastSync = Timestamp.FromDateTimeOffset(lastSyncUtc);
            }

            return Task.FromResult(response);
        }
    }

    public Task<LookupResponse> LookupCallsignAsync(string callsign, CancellationToken ct = default)
    {
        var record = new CallsignRecord
        {
            Callsign = callsign.ToUpperInvariant(),
            FirstName = "Randy",
            LastName = "Treit",
            Nickname = "Randy",
            FormattedName = "Randy Treit",
            LicenseClass = "Extra",
            Addr1 = "1234 Ham Radio Ln",
            Addr2 = "Redmond",
            State = "WA",
            Zip = "98052",
            Country = "United States",
            GridSquare = "CN87",
            County = "King",
            Latitude = 47.6740,
            Longitude = -122.1215,
            Email = "fixture@example.com",
            WebUrl = "https://www.qrz.com/db/" + callsign.ToUpperInvariant(),
            CqZone = 3,
            ItuZone = 2,
            DxccCountryName = "United States",
            DxccContinent = "NA",
            Eqsl = QslPreference.Yes,
            Lotw = QslPreference.Yes,
            PaperQsl = QslPreference.No,
            TimeZone = "America/Los_Angeles",
        };

        record.Aliases.Add("KD7BBJ");

        var result = new LookupResult
        {
            State = LookupState.Found,
            Record = record,
            CacheHit = false,
            LookupLatencyMs = 42,
            QueriedCallsign = callsign,
        };

        return Task.FromResult(new LookupResponse { Result = result });
    }

    public Task<DeleteQsoResponse> DeleteQsoAsync(string localId, bool deleteFromQrz = false, CancellationToken ct = default)
    {
        lock (_gate)
        {
            var index = _recentQsos.FindIndex(item => string.Equals(item.LocalId, localId, StringComparison.Ordinal));
            if (index < 0)
            {
                return Task.FromResult(new DeleteQsoResponse { Success = false, Error = "QSO not found." });
            }

            _recentQsos.RemoveAt(index);
            return Task.FromResult(new DeleteQsoResponse { Success = true });
        }
    }

    public Task<LogQsoResponse> LogQsoAsync(QsoRecord qso, bool syncToQrz = false, CancellationToken ct = default)
    {
        return Task.FromResult(new LogQsoResponse { LocalId = Guid.NewGuid().ToString() });
    }

    public Task<GetRigSnapshotResponse> GetRigSnapshotAsync(CancellationToken ct = default)
    {
        return Task.FromResult(new GetRigSnapshotResponse
        {
            Snapshot = new RigSnapshot
            {
                FrequencyHz = 14225000,
                Band = Band._20M,
                Mode = Mode.Ssb,
                Status = RigConnectionStatus.Connected,
            }
        });
    }

    public Task<GetRigStatusResponse> GetRigStatusAsync(CancellationToken ct = default)
    {
        return Task.FromResult(new GetRigStatusResponse
        {
            Status = RigConnectionStatus.Connected,
        });
    }

    public Task<GetCurrentSpaceWeatherResponse> GetCurrentSpaceWeatherAsync(CancellationToken ct = default)
    {
        return Task.FromResult(new GetCurrentSpaceWeatherResponse
        {
            Snapshot = new SpaceWeatherSnapshot
            {
                PlanetaryKIndex = 2.0,
                SolarFluxIndex = 148.0,
                SunspotNumber = 95,
                Status = SpaceWeatherStatus.Current,
            }
        });
    }

    private SetupStatus BuildSetupStatus()
    {
        var status = new SetupStatus
        {
            ConfigFileExists = _configFileExists,
            SetupComplete = _setupComplete,
            ConfigPath = _configPath,
            HasStationProfile = HasStationProfile(),
            StationProfile = _stationProfile.Clone(),
            StationProfileCount = HasStationProfile() ? 1u : 0u,
            SuggestedLogFilePath = string.IsNullOrWhiteSpace(_logFilePath)
                ? @"C:\Users\Public\QsoRipper\qsoripper.db"
                : _logFilePath,
            IsFirstRun = _isFirstRun,
            HasQrzXmlPassword = _hasQrzXmlPassword,
            HasQrzLogbookApiKey = _hasQrzLogbookApiKey,
            SyncConfig = _syncConfig.Clone()
        };

        if (!string.IsNullOrWhiteSpace(_logFilePath))
        {
            status.LogFilePath = _logFilePath;
        }

        if (!string.IsNullOrWhiteSpace(_qrzXmlUsername))
        {
            status.QrzXmlUsername = _qrzXmlUsername;
        }

        if (HasStationProfile())
        {
            status.ActiveStationProfileId = "capture-profile";
        }

        return status;
    }

    private SetupWizardStepStatus[] BuildStepStatuses() =>
    [
        new SetupWizardStepStatus
        {
            Step = SetupWizardStep.LogFile,
            Complete = !string.IsNullOrWhiteSpace(_logFilePath)
        },
        new SetupWizardStepStatus
        {
            Step = SetupWizardStep.StationProfiles,
            Complete = IsStationProfileComplete(_stationProfile)
        },
        new SetupWizardStepStatus
        {
            Step = SetupWizardStep.QrzIntegration,
            Complete = !string.IsNullOrWhiteSpace(_qrzXmlUsername) && _hasQrzXmlPassword
        },
        new SetupWizardStepStatus
        {
            Step = SetupWizardStep.Review,
            Complete = _setupComplete
        }
    ];

    private static void AddValidation(
        ValidateSetupStepResponse response,
        string field,
        bool valid,
        string message)
    {
        response.Fields.Add(
            new SetupFieldValidation
            {
                Field = field,
                Valid = valid,
                Message = valid ? string.Empty : message
            });
    }

    private static bool IsStationProfileComplete(StationProfile profile)
        => !string.IsNullOrWhiteSpace(profile.ProfileName)
            && !string.IsNullOrWhiteSpace(profile.StationCallsign)
            && !string.IsNullOrWhiteSpace(profile.OperatorCallsign)
            && !string.IsNullOrWhiteSpace(profile.Grid);

    private bool HasStationProfile()
        => !string.IsNullOrWhiteSpace(_stationProfile.StationCallsign)
            || !string.IsNullOrWhiteSpace(_stationProfile.ProfileName);

    private static bool LooksRejected(string value)
        => value.Contains("bad", StringComparison.OrdinalIgnoreCase)
            || value.Contains("fail", StringComparison.OrdinalIgnoreCase)
            || value.Contains("reject", StringComparison.OrdinalIgnoreCase);
}
