namespace LogRipper.Cli;

public static class CliArgumentParser
{
    public const string DefaultEndpoint = "http://localhost:50051";

    public static CliArguments Parse(string[] args)
    {
        var endpoint = Environment.GetEnvironmentVariable("LOGRIPPER_ENDPOINT") ?? DefaultEndpoint;
        string? command = null;

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

            if (arg.StartsWith("-", StringComparison.Ordinal))
            {
                return new CliArguments("help", endpoint, ShowHelp: true, Error: $"Unknown option: {arg}");
            }

            command ??= arg;
        }

        return new CliArguments(command ?? "help", endpoint, ShowHelp: command is null);
    }
}
