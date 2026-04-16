namespace QsoRipper.EngineSelection;

public static class PersistenceSetup
{
    public const string PathKey = "persistence.path";
    public const string LegacyPathEnvironmentVariable = "QSORIPPER_LOG_FILE";

    public static bool HasExplicitMetadata(
        bool contractExplicit,
        bool stepEnabled,
        string? label,
        string? description,
        int definitionCount)
    {
        return contractExplicit
            || stepEnabled
            || !string.IsNullOrWhiteSpace(label)
            || !string.IsNullOrWhiteSpace(description)
            || definitionCount > 0;
    }

    public static bool RequiresPathInput(
        bool contractExplicit,
        bool stepEnabled,
        string? label,
        string? description,
        int definitionCount)
    {
        return !HasExplicitMetadata(contractExplicit, stepEnabled, label, description, definitionCount)
            || stepEnabled;
    }

    public static bool IsPathKey(string? key)
    {
        return string.Equals(key?.Trim(), PathKey, StringComparison.OrdinalIgnoreCase);
    }

    public static string GetEnvironmentVariableName(string key)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(key);

        var normalized = key.Trim();
        const string persistencePrefix = "persistence.";
        if (normalized.StartsWith(persistencePrefix, StringComparison.OrdinalIgnoreCase))
        {
            normalized = normalized[persistencePrefix.Length..];
        }

        var buffer = new System.Text.StringBuilder(normalized.Length);
        foreach (var character in normalized)
        {
            if (char.IsLetterOrDigit(character))
            {
                buffer.Append(char.ToUpperInvariant(character));
            }
            else
            {
                buffer.Append('_');
            }
        }

        var suffix = buffer.ToString().Trim('_');
        return string.IsNullOrWhiteSpace(suffix)
            ? "QSORIPPER_PERSISTENCE"
            : $"QSORIPPER_PERSISTENCE_{suffix}";
    }
}
