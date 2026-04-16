using System.Collections.Generic;
using System.Linq;
using QsoRipper.EngineSelection;
using QsoRipper.Services;

namespace QsoRipper.Shared.Persistence;

internal static class PersistenceSetupFields
{
    private const string DefaultLabel = "Storage";
    private const string DefaultDescription = "Where should QsoRipper store persisted logbook data?";

    public static IReadOnlyList<PersistenceSetupField> FromStatus(SetupStatus? status, string fallbackPath = "")
    {
        if (status is null)
        {
            return [CreateLegacyPathField(null, null, null, null, fallbackPath)];
        }

        if (!PersistenceSetup.HasExplicitMetadata(
                status.PersistenceContractExplicit,
                status.PersistenceStepEnabled,
                status.PersistenceLabel,
                status.PersistenceDescription,
                status.PersistenceDefinitions.Count))
        {
            return
            [
                CreateLegacyPathField(
                    status.PersistenceLabel,
                    status.PersistenceDescription,
                    status.LogFilePath,
                    status.SuggestedLogFilePath,
                    fallbackPath)
            ];
        }

        if (status.PersistenceDefinitions.Count == 0)
        {
            return [];
        }

        var values = status.PersistenceValues.ToDictionary(value => value.Key, StringComparer.OrdinalIgnoreCase);
        var fields = new List<PersistenceSetupField>(status.PersistenceDefinitions.Count);

        foreach (var definition in status.PersistenceDefinitions)
        {
            values.TryGetValue(definition.Key, out var runtimeValue);
            var initialValue = GetInitialValue(status, definition, runtimeValue, fallbackPath);
            fields.Add(
                new PersistenceSetupField
                {
                    Key = definition.Key,
                    Label = string.IsNullOrWhiteSpace(definition.Label) ? definition.Key : definition.Label,
                    Description = NormalizeOptional(definition.Description) ?? string.Empty,
                    Kind = definition.Kind,
                    Required = definition.Required,
                    Secret = definition.Secret || runtimeValue?.Secret == true,
                    HasConfiguredValue = runtimeValue?.HasValue == true,
                    IsRedacted = runtimeValue?.Redacted == true,
                    AllowedValues = definition.AllowedValues.ToArray(),
                    Value = initialValue,
                });
        }

        return fields;
    }

    public static string FormatSummary(SetupStatus? status, string fallbackPath = "")
    {
        if (status is null)
        {
            return "(not configured)";
        }

        var fields = FromStatus(status, fallbackPath);
        if (fields.Count == 0)
        {
            return string.IsNullOrWhiteSpace(status.PersistenceDescription)
                ? "No setup input required"
                : status.PersistenceDescription;
        }

        if (fields.Count == 1 && fields[0].IsPath)
        {
            if (!string.IsNullOrWhiteSpace(fields[0].Value))
            {
                return fields[0].HasConfiguredValue
                    ? fields[0].Value
                    : $"suggested: {fields[0].Value}";
            }

            return "(not configured)";
        }

        return string.Join(
            ", ",
            fields.Select(
                static field => $"{field.Label}: {FormatDisplayValue(field)}"));
    }

    public static string? GetPathValue(IEnumerable<PersistenceSetupField> fields)
    {
        ArgumentNullException.ThrowIfNull(fields);

        return fields
            .Where(static field => field.IsPath)
            .Select(static field => NormalizeFieldValue(field))
            .FirstOrDefault(static value => !string.IsNullOrWhiteSpace(value));
    }

    public static void ApplyTo(SaveSetupRequest request, IEnumerable<PersistenceSetupField> fields)
    {
        ArgumentNullException.ThrowIfNull(request);
        ArgumentNullException.ThrowIfNull(fields);

        ApplyCore(
            fields,
            addValue: request.PersistenceValues.Add,
            setLegacyPath: value => request.LogFilePath = value);
    }

    public static void ApplyTo(ValidateSetupStepRequest request, IEnumerable<PersistenceSetupField> fields)
    {
        ArgumentNullException.ThrowIfNull(request);
        ArgumentNullException.ThrowIfNull(fields);

        ApplyCore(
            fields,
            addValue: request.PersistenceValues.Add,
            setLegacyPath: value => request.LogFilePath = value);
    }

    private static void ApplyCore(
        IEnumerable<PersistenceSetupField> fields,
        Action<SetupFieldValue> addValue,
        Action<string> setLegacyPath)
    {
        foreach (var field in fields)
        {
            var normalizedValue = NormalizeFieldValue(field);
            if (string.IsNullOrWhiteSpace(normalizedValue))
            {
                continue;
            }

            addValue(
                new SetupFieldValue
                {
                    Key = field.Key,
                    Value = normalizedValue,
                });

            if (field.IsPath)
            {
                setLegacyPath(normalizedValue);
            }
        }
    }

    private static PersistenceSetupField CreateLegacyPathField(
        string? label,
        string? description,
        string? currentPath,
        string? suggestedPath,
        string fallbackPath)
    {
        var configuredPath = NormalizeOptional(currentPath);
        var suggested = configuredPath
            ?? NormalizeOptional(suggestedPath)
            ?? NormalizeOptional(fallbackPath)
            ?? string.Empty;

        return new PersistenceSetupField
        {
            Key = PersistenceSetup.PathKey,
            Label = string.IsNullOrWhiteSpace(label) ? DefaultLabel : label,
            Description = string.IsNullOrWhiteSpace(description) ? DefaultDescription : description,
            Kind = RuntimeConfigValueKind.Path,
            Required = true,
            HasConfiguredValue = !string.IsNullOrWhiteSpace(configuredPath),
            Value = suggested,
        };
    }

    private static string GetInitialValue(
        SetupStatus status,
        RuntimeConfigDefinition definition,
        RuntimeConfigValue? runtimeValue,
        string fallbackPath)
    {
        if (runtimeValue?.HasValue == true && !(runtimeValue.Secret || runtimeValue.Redacted))
        {
            return runtimeValue.DisplayValue ?? string.Empty;
        }

        if (PersistenceSetup.IsPathKey(definition.Key) || definition.Kind == RuntimeConfigValueKind.Path)
        {
            return NormalizeOptional(status.LogFilePath)
                ?? NormalizeOptional(status.SuggestedLogFilePath)
                ?? NormalizeOptional(fallbackPath)
                ?? string.Empty;
        }

        return string.Empty;
    }

    private static string FormatDisplayValue(PersistenceSetupField field)
    {
        if (field.Secret || field.IsRedacted)
        {
            return field.HasConfiguredValue ? "(configured)" : "(not set)";
        }

        return string.IsNullOrWhiteSpace(field.Value)
            ? "(not set)"
            : field.Value;
    }

    private static string NormalizeFieldValue(PersistenceSetupField field)
    {
        var trimmed = field.Value?.Trim() ?? string.Empty;
        if (string.IsNullOrWhiteSpace(trimmed))
        {
            return string.Empty;
        }

        if (field.HasChoices)
        {
            var canonicalChoice = field.AllowedValues.FirstOrDefault(
                allowed => string.Equals(allowed, trimmed, StringComparison.OrdinalIgnoreCase));
            if (!string.IsNullOrWhiteSpace(canonicalChoice))
            {
                return canonicalChoice;
            }
        }

        if (field.Kind == RuntimeConfigValueKind.Boolean
            && bool.TryParse(trimmed, out var parsedBoolean))
        {
            return parsedBoolean ? "true" : "false";
        }

        return trimmed;
    }

    private static string? NormalizeOptional(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value.Trim();
    }
}
