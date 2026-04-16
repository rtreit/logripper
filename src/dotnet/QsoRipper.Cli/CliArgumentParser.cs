using QsoRipper.EngineSelection;

namespace QsoRipper.Cli;

internal static class CliArgumentParser
{
    public static string DefaultEndpoint => EngineCatalog.DefaultRustEndpoint;

    public static CliArguments Parse(string[] args)
    {
        ArgumentNullException.ThrowIfNull(args);

        var engineProfile = EngineCatalog.ResolveProfile();
        var endpoint = EngineCatalog.ResolveEndpoint(engineProfile);
        var hasExplicitEndpoint = false;
        string? command = null;
        string? callsign = null;
        var skipCache = false;
        var jsonOutput = false;
        var refresh = false;
        var force = false;
        var setupStatus = false;
        var setupFromEnv = false;

        var remaining = new List<string>();

        for (var i = 0; i < args.Length; i++)
        {
            var arg = args[i];

            if (arg is "--help" or "-h" or "help")
            {
                if (command is null)
                {
                    return new CliArguments("help", endpoint, engineProfile, ShowHelp: true);
                }

                remaining.Add(arg);
                continue;
            }

            if (arg == "--engine")
            {
                if (i == args.Length - 1)
                {
                    return new CliArguments(
                        "help",
                        endpoint,
                        engineProfile,
                        ShowHelp: true,
                        Error: "Missing value for --engine.");
                }

                if (!EngineCatalog.TryResolveProfile(args[++i], out var parsedProfile))
                {
                    return new CliArguments(
                        "help",
                        endpoint,
                        engineProfile,
                        ShowHelp: true,
                        Error: $"Unknown engine profile '{args[i]}'. Known values: {EngineCatalog.GetKnownProfileList()}.");
                }

                engineProfile = parsedProfile;
                if (!hasExplicitEndpoint)
                {
                    endpoint = EngineCatalog.ResolveEndpoint(engineProfile);
                }

                continue;
            }

            if (arg is "--endpoint" or "-e")
            {
                if (i == args.Length - 1)
                {
                    return new CliArguments(
                        "help",
                        endpoint,
                        engineProfile,
                        ShowHelp: true,
                        Error: "Missing value for --endpoint.");
                }

                endpoint = args[++i];
                hasExplicitEndpoint = true;
                continue;
            }

            if (arg is "--skip-cache")
            {
                skipCache = true;
                continue;
            }

            if (arg is "--refresh")
            {
                refresh = true;
                continue;
            }

            if (arg is "--force")
            {
                force = true;
                continue;
            }

            if (arg is "--json")
            {
                jsonOutput = true;
                continue;
            }

            if (arg is "--status")
            {
                setupStatus = true;
                continue;
            }

            if (arg is "--from-env")
            {
                setupFromEnv = true;
                continue;
            }

            if (command is null && !arg.StartsWith('-'))
            {
                command = arg;
                continue;
            }

            var commandNeedsCallsign = CliCommandMetadata.UsesPrimaryArgument(command);

            if (command is not null && callsign is null && !arg.StartsWith('-') && commandNeedsCallsign)
            {
                callsign = arg;
                continue;
            }

            remaining.Add(arg);
        }

        return new CliArguments(
            command ?? "help",
            endpoint,
            engineProfile,
            ShowHelp: command is null,
            Callsign: callsign,
            SkipCache: skipCache,
            JsonOutput: jsonOutput,
            Refresh: refresh,
            Force: force,
            SetupStatus: setupStatus,
            SetupFromEnv: setupFromEnv,
            RemainingArgs: remaining.ToArray());
    }
}
