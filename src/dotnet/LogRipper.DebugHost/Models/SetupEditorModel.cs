using System.ComponentModel.DataAnnotations;
using LogRipper.Services;

namespace LogRipper.DebugHost.Models;

internal sealed class SetupEditorModel : StationProfileEditorModelBase
{
    public string LogFilePath { get; set; } = @".\data\logripper.db";

    public string? QrzXmlUsername { get; set; }

    public string? QrzXmlPassword { get; set; }

    public static SetupEditorModel Create(SetupStatus? status, string fallbackLogFilePath)
    {
        var model = new SetupEditorModel
        {
            LogFilePath = NormalizeOptional(status?.LogFilePath)
                ?? NormalizeOptional(status?.SuggestedLogFilePath)
                ?? fallbackLogFilePath,
            QrzXmlUsername = NormalizeOptional(status?.QrzXmlUsername)
        };

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

        return request;
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
