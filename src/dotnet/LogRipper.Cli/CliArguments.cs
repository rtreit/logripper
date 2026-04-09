namespace LogRipper.Cli;

public sealed record CliArguments(
    string Command,
    string Endpoint,
    bool ShowHelp = false,
    string? Error = null);
