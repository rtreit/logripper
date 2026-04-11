using System.ComponentModel.DataAnnotations;
using LogRipper.Services;

namespace LogRipper.DebugHost.Models;

internal sealed class SetupEditorModel : StationProfileEditorModelBase
{
    public StorageBackend StorageBackend { get; set; } = StorageBackend.Memory;

    public string SqlitePath { get; set; } = @".\data\logripper.db";

    public string? QrzXmlUsername { get; set; }

    public string? QrzXmlPassword { get; set; }

    public static SetupEditorModel Create(SetupStatusResponse? status, EngineStorageBackend fallbackStorageBackend, string fallbackSqlitePath)
    {
        var model = new SetupEditorModel
        {
            StorageBackend = status?.StorageBackend switch
            {
                StorageBackend.Sqlite => StorageBackend.Sqlite,
                StorageBackend.Memory => StorageBackend.Memory,
                _ => fallbackStorageBackend == EngineStorageBackend.Sqlite
                    ? StorageBackend.Sqlite
                    : StorageBackend.Memory
            },
            SqlitePath = status?.SqlitePath
                ?? status?.SuggestedSqlitePath
                ?? fallbackSqlitePath,
            QrzXmlUsername = NormalizeOptional(status?.QrzXmlUsername)
        };

        model.LoadFrom(status?.StationProfile);
        return model;
    }

    public SaveSetupRequest ToRequest()
    {
        var request = new SaveSetupRequest
        {
            StorageBackend = StorageBackend,
            StationProfile = ToStationProfile()
        };

        if (!string.IsNullOrWhiteSpace(SqlitePath))
        {
            request.SqlitePath = SqlitePath.Trim();
        }

        if (!string.IsNullOrWhiteSpace(QrzXmlUsername))
        {
            request.QrzXmlUsername = QrzXmlUsername.Trim();
        }

        if (!string.IsNullOrWhiteSpace(QrzXmlPassword))
        {
            request.QrzXmlPassword = QrzXmlPassword.Trim();
        }

        return request;
    }

    public override IEnumerable<ValidationResult> Validate(ValidationContext validationContext)
    {
        foreach (var result in base.Validate(validationContext))
        {
            yield return result;
        }

        if (StorageBackend == StorageBackend.Sqlite && string.IsNullOrWhiteSpace(SqlitePath))
        {
            yield return new ValidationResult(
                "SQLite path is required when SQLite storage is selected.",
                [nameof(SqlitePath)]);
        }

        if (string.IsNullOrWhiteSpace(QrzXmlUsername) != string.IsNullOrWhiteSpace(QrzXmlPassword))
        {
            yield return new ValidationResult(
                "QRZ XML username and password must either both be set or both be blank.",
                [nameof(QrzXmlUsername), nameof(QrzXmlPassword)]);
        }
    }
}
