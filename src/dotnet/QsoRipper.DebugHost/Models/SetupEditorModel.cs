using System.ComponentModel.DataAnnotations;
using System.Linq;
using QsoRipper.Services;
using QsoRipper.Shared.Persistence;

namespace QsoRipper.DebugHost.Models;

internal sealed class SetupEditorModel : StationProfileEditorModelBase
{
    public string PersistenceDescription { get; set; } = "Where should QsoRipper store persisted logbook data?";

    public string PersistenceLabel { get; set; } = "Storage";

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

    public List<PersistenceSetupField> PersistenceFields { get; } = [];

    private bool HasPersistedRigControl { get; set; }

    public bool HasPersistenceInputs => PersistenceFields.Count > 0;

    public bool RequiresLogFilePath
    {
        get => PersistenceFields.Count == 1 && PersistenceFields[0].IsPath;
        set
        {
            if (value)
            {
                if (PersistenceFields.Count == 0)
                {
                    ReplacePersistenceFields(
                    [
                        new PersistenceSetupField
                        {
                            Key = QsoRipper.EngineSelection.PersistenceSetup.PathKey,
                            Label = "Path",
                            Description = PersistenceDescription,
                            Kind = RuntimeConfigValueKind.Path,
                            Required = true,
                            Value = LogFilePath,
                        }
                    ]);
                }
            }
            else
            {
                PersistenceFields.Clear();
            }
        }
    }

    public string LogFilePath
    {
        get => PersistenceSetupFields.GetPathValue(PersistenceFields) ?? string.Empty;
        set
        {
            if (PersistenceFields.FirstOrDefault(persistenceField => persistenceField.IsPath) is { } pathField)
            {
                pathField.Value = value;
            }
        }
    }

    public static SetupEditorModel Create(SetupStatus? status, string fallbackLogFilePath)
    {
        var model = new SetupEditorModel
        {
            PersistenceDescription = NormalizeOptional(status?.PersistenceDescription)
                ?? "Where should QsoRipper store persisted logbook data?",
            PersistenceLabel = NormalizeOptional(status?.PersistenceLabel)
                ?? "Storage",
            QrzXmlUsername = NormalizeOptional(status?.QrzXmlUsername),
            HasPersistedRigControl = status?.RigControl is not null
        };
        model.ReplacePersistenceFields(PersistenceSetupFields.FromStatus(status, fallbackLogFilePath));

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

        PersistenceSetupFields.ApplyTo(request, PersistenceFields);

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

        foreach (var field in PersistenceFields.Where(static field => field.Required && string.IsNullOrWhiteSpace(field.Value)))
        {
            yield return new ValidationResult(
                $"{field.Label} is required.",
                [field.IsPath && RequiresLogFilePath ? nameof(LogFilePath) : nameof(PersistenceFields)]);
        }

        if (string.IsNullOrWhiteSpace(QrzXmlUsername) != string.IsNullOrWhiteSpace(QrzXmlPassword))
        {
            yield return new ValidationResult(
                "QRZ XML username and password must either both be set or both be blank.",
                [nameof(QrzXmlUsername), nameof(QrzXmlPassword)]);
        }
    }

    private void ReplacePersistenceFields(IEnumerable<PersistenceSetupField> fields)
    {
        PersistenceFields.Clear();
        PersistenceFields.AddRange(fields);
    }
}
