using System;
using System.Globalization;
using System.Linq;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using QsoRipper.Domain;
using QsoRipper.Gui.Services;
using QsoRipper.Services;

namespace QsoRipper.Gui.ViewModels;

internal sealed partial class SettingsViewModel : ObservableObject
{
    private readonly IEngineClient _engine;

    // Station Profile
    [ObservableProperty]
    private string _callsign = string.Empty;

    [ObservableProperty]
    private string _gridSquare = string.Empty;

    [ObservableProperty]
    private string _operatorName = string.Empty;

    [ObservableProperty]
    private string _operatorCallsign = string.Empty;

    [ObservableProperty]
    private string _profileName = string.Empty;

    [ObservableProperty]
    private string _county = string.Empty;

    [ObservableProperty]
    private string _state = string.Empty;

    [ObservableProperty]
    private string _country = string.Empty;

    [ObservableProperty]
    private string _dxcc = string.Empty;

    [ObservableProperty]
    private string _cqZone = string.Empty;

    [ObservableProperty]
    private string _ituZone = string.Empty;

    [ObservableProperty]
    private string _latitude = string.Empty;

    [ObservableProperty]
    private string _longitude = string.Empty;

    [ObservableProperty]
    private string _arrlSection = string.Empty;

    // QRZ XML
    [ObservableProperty]
    private string _qrzXmlUsername = string.Empty;

    [ObservableProperty]
    private string _qrzXmlPassword = string.Empty;

    [ObservableProperty]
    private bool _isTestingQrzXml;

    [ObservableProperty]
    private string? _qrzXmlTestResult;

    [ObservableProperty]
    private bool _qrzXmlTestSucceeded;

    // QRZ Logbook
    [ObservableProperty]
    private string _qrzLogbookApiKey = string.Empty;

    [ObservableProperty]
    private bool _isTestingLogbook;

    [ObservableProperty]
    private string? _logbookTestResult;

    [ObservableProperty]
    private bool _logbookTestSucceeded;

    // Sync Settings
    [ObservableProperty]
    private bool _autoSyncEnabled;

    [ObservableProperty]
    private int _syncIntervalSeconds = 300;

    [ObservableProperty]
    private ConflictPolicy _conflictPolicy = ConflictPolicy.LastWriteWins;

    // Rig control
    [ObservableProperty]
    private bool _rigControlEnabled;

    [ObservableProperty]
    private string _rigControlHost = string.Empty;

    [ObservableProperty]
    private string _rigControlPort = string.Empty;

    [ObservableProperty]
    private string _rigControlReadTimeoutMs = string.Empty;

    [ObservableProperty]
    private string _rigControlStaleThresholdMs = string.Empty;

    // Log file (read-only display)
    [ObservableProperty]
    private string _logFilePath = string.Empty;

    // UI state
    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private bool _isSaving;

    [ObservableProperty]
    private string? _errorMessage;

    /// <summary>
    /// True after a successful save. Checked by the caller after the dialog closes.
    /// </summary>
    [ObservableProperty]
    private bool _didSave;

    private bool _hasPersistedRigControl;

    /// <summary>
    /// Raised when the dialog should close. The bool parameter is true for save, false for cancel.
    /// </summary>
    internal event EventHandler<bool>? CloseRequested;

    public SettingsViewModel(IEngineClient engine)
    {
        _engine = engine;
    }

    /// <summary>
    /// Loads current settings from the engine. Call after construction.
    /// </summary>
    internal async Task LoadAsync()
    {
        IsLoading = true;
        ErrorMessage = null;
        try
        {
            var state = await _engine.GetWizardStateAsync();
            ApplyStatus(state.Status);

            var activeProfile = state.StationProfiles
                .FirstOrDefault(p => p.IsActive)?.Profile
                ?? state.Status.StationProfile;

            if (activeProfile is not null)
            {
                ApplyStationProfile(activeProfile);
            }
        }
        catch (Grpc.Core.RpcException ex)
        {
            ErrorMessage = $"Failed to load settings: {ex.Status.Detail}";
        }
        finally
        {
            IsLoading = false;
        }
    }

    [RelayCommand]
    private async Task SaveAsync()
    {
        IsSaving = true;
        ErrorMessage = null;
        try
        {
            if (!TryValidateRigControlInputs(out var validationError))
            {
                ErrorMessage = validationError;
                return;
            }

            var request = BuildSaveRequest();
            await _engine.SaveSetupAsync(request);
            DidSave = true;
            CloseRequested?.Invoke(this, true);
        }
        catch (Grpc.Core.RpcException ex)
        {
            ErrorMessage = $"Save failed: {ex.Status.Detail}";
        }
        finally
        {
            IsSaving = false;
        }
    }

    [RelayCommand]
    private void Cancel()
    {
        CloseRequested?.Invoke(this, false);
    }

    [RelayCommand]
    private async Task TestQrzXmlAsync()
    {
        if (string.IsNullOrWhiteSpace(QrzXmlUsername) || string.IsNullOrWhiteSpace(QrzXmlPassword))
        {
            QrzXmlTestResult = "Username and password are required.";
            QrzXmlTestSucceeded = false;
            return;
        }

        IsTestingQrzXml = true;
        QrzXmlTestResult = null;
        try
        {
            var result = await _engine.TestQrzCredentialsAsync(QrzXmlUsername, QrzXmlPassword);
            QrzXmlTestSucceeded = result.Success;
            QrzXmlTestResult = result.Success
                ? "✓ Connected to QRZ XML successfully!"
                : $"✗ {result.ErrorMessage}";
        }
        catch (Grpc.Core.RpcException ex)
        {
            QrzXmlTestSucceeded = false;
            QrzXmlTestResult = $"✗ Connection failed: {ex.Status.Detail}";
        }
        finally
        {
            IsTestingQrzXml = false;
        }
    }

    [RelayCommand]
    private async Task TestQrzLogbookAsync()
    {
        if (string.IsNullOrWhiteSpace(QrzLogbookApiKey))
        {
            LogbookTestResult = "API key is required.";
            LogbookTestSucceeded = false;
            return;
        }

        IsTestingLogbook = true;
        LogbookTestResult = null;
        try
        {
            var result = await _engine.TestQrzLogbookCredentialsAsync(QrzLogbookApiKey);
            LogbookTestSucceeded = result.Success;
            if (result.Success)
            {
                var owner = string.IsNullOrWhiteSpace(result.LogbookOwner)
                    ? string.Empty
                    : $" ({result.LogbookOwner})";
                var count = result.HasQsoCount
                    ? $" — {result.QsoCount} QSOs"
                    : string.Empty;
                LogbookTestResult = $"✓ Logbook connected{owner}{count}";
            }
            else
            {
                LogbookTestResult = $"✗ {result.ErrorMessage}";
            }
        }
        catch (Grpc.Core.RpcException ex)
        {
            LogbookTestSucceeded = false;
            LogbookTestResult = $"✗ Connection failed: {ex.Status.Detail}";
        }
        finally
        {
            IsTestingLogbook = false;
        }
    }

    private void ApplyStatus(SetupStatus status)
    {
        QrzXmlUsername = status.QrzXmlUsername ?? string.Empty;
        LogFilePath = status.LogFilePath ?? string.Empty;
        _hasPersistedRigControl = status.RigControl is not null;

        if (status.SyncConfig is not null)
        {
            AutoSyncEnabled = status.SyncConfig.AutoSyncEnabled;
            SyncIntervalSeconds = status.SyncConfig.SyncIntervalSeconds > 0
                ? (int)status.SyncConfig.SyncIntervalSeconds
                : 300;
            ConflictPolicy = status.SyncConfig.ConflictPolicy;
        }

        if (status.RigControl is not null)
        {
            RigControlEnabled = status.RigControl.Enabled;
            RigControlHost = status.RigControl.HasHost
                ? status.RigControl.Host
                : string.Empty;
            RigControlPort = status.RigControl.HasPort
                ? status.RigControl.Port.ToString(CultureInfo.InvariantCulture)
                : string.Empty;
            RigControlReadTimeoutMs = status.RigControl.HasReadTimeoutMs
                ? status.RigControl.ReadTimeoutMs.ToString(CultureInfo.InvariantCulture)
                : string.Empty;
            RigControlStaleThresholdMs = status.RigControl.HasStaleThresholdMs
                ? status.RigControl.StaleThresholdMs.ToString(CultureInfo.InvariantCulture)
                : string.Empty;
        }
        else
        {
            RigControlEnabled = false;
            RigControlHost = string.Empty;
            RigControlPort = string.Empty;
            RigControlReadTimeoutMs = string.Empty;
            RigControlStaleThresholdMs = string.Empty;
        }

        // Password and API key are never returned by the engine for security;
        // leave them empty so the user can re-enter if they want to change them.
    }

    private void ApplyStationProfile(StationProfile profile)
    {
        ProfileName = profile.ProfileName ?? string.Empty;
        Callsign = profile.StationCallsign ?? string.Empty;
        OperatorCallsign = profile.OperatorCallsign ?? string.Empty;
        OperatorName = profile.OperatorName ?? string.Empty;
        GridSquare = profile.Grid ?? string.Empty;
        County = profile.County ?? string.Empty;
        State = profile.State ?? string.Empty;
        Country = profile.Country ?? string.Empty;
        ArrlSection = profile.ArrlSection ?? string.Empty;
        Dxcc = profile.Dxcc != 0
            ? profile.Dxcc.ToString(CultureInfo.InvariantCulture) : string.Empty;
        CqZone = profile.CqZone != 0
            ? profile.CqZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
        ItuZone = profile.ItuZone != 0
            ? profile.ItuZone.ToString(CultureInfo.InvariantCulture) : string.Empty;
        Latitude = profile.Latitude != 0
            ? profile.Latitude.ToString(CultureInfo.InvariantCulture) : string.Empty;
        Longitude = profile.Longitude != 0
            ? profile.Longitude.ToString(CultureInfo.InvariantCulture) : string.Empty;
    }

    private SaveSetupRequest BuildSaveRequest()
    {
        var profile = new StationProfile
        {
            StationCallsign = Callsign.Trim(),
            Grid = GridSquare.Trim(),
            OperatorName = OperatorName.Trim(),
        };

        SetOptionalString(ProfileName, v => profile.ProfileName = v);
        SetOptionalString(OperatorCallsign, v => profile.OperatorCallsign = v);
        SetOptionalString(County, v => profile.County = v);
        SetOptionalString(State, v => profile.State = v);
        SetOptionalString(Country, v => profile.Country = v);
        SetOptionalString(ArrlSection, v => profile.ArrlSection = v);
        SetUintField(Dxcc, v => profile.Dxcc = v);
        SetUintField(CqZone, v => profile.CqZone = v);
        SetUintField(ItuZone, v => profile.ItuZone = v);
        SetDoubleField(Latitude, v => profile.Latitude = v);
        SetDoubleField(Longitude, v => profile.Longitude = v);

        var request = new SaveSetupRequest
        {
            StationProfile = profile,
            LogFilePath = LogFilePath.Trim(),
            SyncConfig = new SyncConfig
            {
                AutoSyncEnabled = AutoSyncEnabled,
                SyncIntervalSeconds = SyncIntervalSeconds > 0
                    ? (uint)SyncIntervalSeconds : 300,
                ConflictPolicy = ConflictPolicy,
            },
        };

        if (!string.IsNullOrWhiteSpace(QrzXmlUsername))
        {
            request.QrzXmlUsername = QrzXmlUsername.Trim();
            if (!string.IsNullOrEmpty(QrzXmlPassword))
            {
                request.QrzXmlPassword = QrzXmlPassword;
            }
        }

        if (!string.IsNullOrWhiteSpace(QrzLogbookApiKey))
        {
            request.QrzLogbookApiKey = QrzLogbookApiKey.Trim();
        }

        var rigControl = BuildRigControlSettings();
        if (rigControl is not null)
        {
            request.RigControl = rigControl;
        }

        return request;
    }

    private RigControlSettings? BuildRigControlSettings()
    {
        var hasExplicitValues = RigControlEnabled
            || !string.IsNullOrWhiteSpace(RigControlHost)
            || !string.IsNullOrWhiteSpace(RigControlPort)
            || !string.IsNullOrWhiteSpace(RigControlReadTimeoutMs)
            || !string.IsNullOrWhiteSpace(RigControlStaleThresholdMs);

        if (!_hasPersistedRigControl && !hasExplicitValues)
        {
            return null;
        }

        var settings = new RigControlSettings();
        if (_hasPersistedRigControl || RigControlEnabled)
        {
            settings.Enabled = RigControlEnabled;
        }

        SetOptionalString(RigControlHost, value => settings.Host = value);
        SetUInt32Field(RigControlPort, value => settings.Port = value);
        SetUInt64Field(RigControlReadTimeoutMs, value => settings.ReadTimeoutMs = value);
        SetUInt64Field(RigControlStaleThresholdMs, value => settings.StaleThresholdMs = value);
        return settings;
    }

    private bool TryValidateRigControlInputs(out string? validationError)
    {
        if (!TryValidateUInt32Field(
                RigControlPort,
                1,
                65_535,
                "Rig control port",
                out validationError))
        {
            return false;
        }

        if (!TryValidateUInt64Field(
                RigControlReadTimeoutMs,
                1,
                "Rig control read timeout",
                out validationError))
        {
            return false;
        }

        return TryValidateUInt64Field(
            RigControlStaleThresholdMs,
            1,
            "Rig control stale threshold",
            out validationError);
    }

    private static void SetOptionalString(string input, Action<string> setter)
    {
        if (!string.IsNullOrWhiteSpace(input))
        {
            setter(input.Trim());
        }
    }

    private static void SetUintField(string? input, Action<uint> setter)
    {
        if (uint.TryParse(input, CultureInfo.InvariantCulture, out var value))
        {
            setter(value);
        }
    }

    private static void SetUInt32Field(string? input, Action<uint> setter)
    {
        if (uint.TryParse(input, CultureInfo.InvariantCulture, out var value))
        {
            setter(value);
        }
    }

    private static void SetUInt64Field(string? input, Action<ulong> setter)
    {
        if (ulong.TryParse(input, CultureInfo.InvariantCulture, out var value))
        {
            setter(value);
        }
    }

    private static bool TryValidateUInt32Field(
        string? input,
        uint min,
        uint max,
        string label,
        out string? errorMessage)
    {
        errorMessage = null;
        if (string.IsNullOrWhiteSpace(input))
        {
            return true;
        }

        if (!uint.TryParse(input, CultureInfo.InvariantCulture, out var value)
            || value < min
            || value > max)
        {
            errorMessage = $"{label} must be a whole number between {min} and {max}.";
            return false;
        }

        return true;
    }

    private static bool TryValidateUInt64Field(
        string? input,
        ulong min,
        string label,
        out string? errorMessage)
    {
        errorMessage = null;
        if (string.IsNullOrWhiteSpace(input))
        {
            return true;
        }

        if (!ulong.TryParse(input, CultureInfo.InvariantCulture, out var value) || value < min)
        {
            errorMessage = $"{label} must be a whole number greater than or equal to {min}.";
            return false;
        }

        return true;
    }

    private static void SetDoubleField(string? input, Action<double> setter)
    {
        if (double.TryParse(input, CultureInfo.InvariantCulture, out var value))
        {
            setter(value);
        }
    }
}
