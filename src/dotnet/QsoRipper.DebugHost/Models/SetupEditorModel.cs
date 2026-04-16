using System.ComponentModel.DataAnnotations;
using QsoRipper.Services;

namespace QsoRipper.DebugHost.Models;

internal sealed class SetupEditorModel : StationProfileEditorModelBase
{
    public string LogFilePath { get; set; } = @".\data\qsoripper.db";

    public string? QrzXmlUsername { get; set; }

    public string? QrzXmlPassword { get; set; }

    public bool RigControlEnabled { get; set; }

    public string? RigControlHost { get; set; }

    [Range(1, 65535)]
    public int? RigControlPort { get; set; }

    [Range(1, long.MaxValue)]
    public long? RigControlReadTimeoutMs { get; set; }

    [Range(1, long.MaxValue)]
    public long? RigControlStaleThresholdMs { get; set; }

    private bool HasPersistedRigControl { get; set; }

    public static SetupEditorModel Create(SetupStatus? status, string fallbackLogFilePath)
    {
        var model = new SetupEditorModel
        {
            LogFilePath = NormalizeOptional(status?.LogFilePath)
                ?? NormalizeOptional(status?.SuggestedLogFilePath)
                ?? fallbackLogFilePath,
            QrzXmlUsername = NormalizeOptional(status?.QrzXmlUsername),
            HasPersistedRigControl = status?.RigControl is not null
        };

        if (status?.RigControl is not null)
        {
            model.RigControlEnabled = status.RigControl.Enabled;
            model.RigControlHost = status.RigControl.HasHost
                ? NormalizeOptional(status.RigControl.Host)
                : null;
            model.RigControlPort = status.RigControl.HasPort
                ? (int)status.RigControl.Port
                : null;
            model.RigControlReadTimeoutMs = status.RigControl.HasReadTimeoutMs
                ? (long)status.RigControl.ReadTimeoutMs
                : null;
            model.RigControlStaleThresholdMs = status.RigControl.HasStaleThresholdMs
                ? (long)status.RigControl.StaleThresholdMs
                : null;
        }

        model.LoadFrom(status?.StationProfile);
        return model;
    }

    public SaveSetupRequest ToRequest()
    {
        var request = new SaveSetupRequest
        {
            StationProfile = ToStationProfile()
        };

        if (!string.IsNullOrWhiteSpace(LogFilePath))
        {
            request.LogFilePath = LogFilePath.Trim();
        }

        if (!string.IsNullOrWhiteSpace(QrzXmlUsername))
        {
            request.QrzXmlUsername = QrzXmlUsername.Trim();
        }

        if (!string.IsNullOrWhiteSpace(QrzXmlPassword))
        {
            request.QrzXmlPassword = QrzXmlPassword.Trim();
        }

        var rigControl = BuildRigControlRequest();
        if (rigControl is not null)
        {
            request.RigControl = rigControl;
        }

        return request;
    }

    private RigControlSettings? BuildRigControlRequest()
    {
        var hasExplicitValues = RigControlEnabled
            || !string.IsNullOrWhiteSpace(RigControlHost)
            || RigControlPort.HasValue
            || RigControlReadTimeoutMs.HasValue
            || RigControlStaleThresholdMs.HasValue;

        if (!HasPersistedRigControl && !hasExplicitValues)
        {
            return null;
        }

        var settings = new RigControlSettings();
        if (HasPersistedRigControl || RigControlEnabled)
        {
            settings.Enabled = RigControlEnabled;
        }

        if (!string.IsNullOrWhiteSpace(RigControlHost))
        {
            settings.Host = RigControlHost.Trim();
        }

        if (RigControlPort.HasValue)
        {
            settings.Port = (uint)RigControlPort.Value;
        }

        if (RigControlReadTimeoutMs.HasValue)
        {
            settings.ReadTimeoutMs = (ulong)RigControlReadTimeoutMs.Value;
        }

        if (RigControlStaleThresholdMs.HasValue)
        {
            settings.StaleThresholdMs = (ulong)RigControlStaleThresholdMs.Value;
        }

        return settings;
    }

    public override IEnumerable<ValidationResult> Validate(ValidationContext validationContext)
    {
        foreach (var result in base.Validate(validationContext))
        {
            yield return result;
        }

        if (string.IsNullOrWhiteSpace(LogFilePath))
        {
            yield return new ValidationResult(
                "Log file path is required.",
                [nameof(LogFilePath)]);
        }

        if (string.IsNullOrWhiteSpace(QrzXmlUsername) != string.IsNullOrWhiteSpace(QrzXmlPassword))
        {
            yield return new ValidationResult(
                "QRZ XML username and password must either both be set or both be blank.",
                [nameof(QrzXmlUsername), nameof(QrzXmlPassword)]);
        }
    }
}
