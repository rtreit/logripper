using System.Linq;

namespace QsoRipper.EngineSelection;

public sealed record EngineTargetProfile(
    string ProfileId,
    string EngineId,
    string DisplayName,
    string DefaultEndpoint,
    IReadOnlyList<string> Aliases,
    EngineLaunchRecipe? LocalLaunchRecipe)
{
    public bool Matches(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return false;
        }

        var candidate = value.Trim();
        return string.Equals(ProfileId, candidate, StringComparison.OrdinalIgnoreCase)
            || string.Equals(EngineId, candidate, StringComparison.OrdinalIgnoreCase)
            || Aliases.Any(alias => string.Equals(alias, candidate, StringComparison.OrdinalIgnoreCase));
    }
}
