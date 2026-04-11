namespace LogRipper.Cli;

internal static class CliArgumentParser
{
    public const string DefaultEndpoint = "http://127.0.0.1:50051";

    public static CliArguments Parse(string[] args)
    {
        ArgumentNullException.ThrowIfNull(args);

        var endpoint = Environment.GetEnvironmentVariable("LOGRIPPER_ENDPOINT") ?? DefaultEndpoint;
        string? command = null;
        string? callsign = null;
        var skipCache = false;

        var remaining = new List<string>();

        for (var i = 0; i < args.Length; i++)
        {
            var arg = args[i];

            if (arg is "--help" or "-h" or "help")
            {
                return new CliArguments("help", endpoint, ShowHelp: true);
            }

            if (arg is "--endpoint" or "-e")
            {
                if (i == args.Length - 1)
                {
                    return new CliArguments("help", endpoint, ShowHelp: true, Error: "Missing value for --endpoint.");
                }

                endpoint = args[++i];
                continue;
            }

            if (arg is "--skip-cache")
            {
                skipCache = true;
                continue;
            }

            if (command is null && !arg.StartsWith('-'))
            {
                command = arg;
                continue;
            }

            if (command is not null && callsign is null && !arg.StartsWith('-'))
            {
                callsign = arg.ToUpperInvariant();
                continue;
            }

            remaining.Add(arg);
        }

        return new CliArguments(
            command ?? "help",
            endpoint,
            ShowHelp: command is null,
            Callsign: callsign,
            SkipCache: skipCache,
            RemainingArgs: remaining.ToArray());
    }
}
