namespace QsoRipper.DebugHost.Models;

internal sealed record CommandExecutionResult(
    DebugCommandDefinition Command,
    int ExitCode,
    string StandardOutput,
    string StandardError,
    DateTimeOffset StartedAtUtc,
    DateTimeOffset CompletedAtUtc,
    IReadOnlyDictionary<string, string> EffectiveEnvironment)
{
    public bool Succeeded => ExitCode == 0;

    public TimeSpan Duration => CompletedAtUtc - StartedAtUtc;
}
