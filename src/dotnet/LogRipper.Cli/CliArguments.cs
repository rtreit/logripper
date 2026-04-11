namespace LogRipper.Cli;

internal sealed record CliArguments(
    string Command,
    string Endpoint,
    bool ShowHelp = false,
    string? Error = null,
    string? Callsign = null,
    bool SkipCache = false);
