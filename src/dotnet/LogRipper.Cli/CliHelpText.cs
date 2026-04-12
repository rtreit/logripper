namespace LogRipper.Cli;

internal static class CliHelpText
{
    public static string GetGeneralHelp()
    {
        return """
            LogRipper CLI

            Usage: logripper-cli [options] <command> [arguments]

            Logbook:
              log <call> <band> <mode>         Log a QSO (e.g., log W1AW 20m FT8)
              get <local-id>                   Get a QSO by ID
              list [filters]                   List QSOs (--callsign, --band, --mode, --limit)
              delete <local-id>                Delete a QSO

            ADIF:
              import <file>                    Import QSOs from an ADIF file
              export [--file out.adi]          Export QSOs to ADIF (stdout or file)

            Lookup:
              lookup <callsign>                Look up a callsign via QRZ
              stream-lookup <callsign>         Streaming lookup with progressive updates
              cache-check <callsign>           Check if a callsign is cached

            Engine:
              status                           Show sync status and QSO counts
              config [--set KEY=VALUE]         View or modify runtime config
              setup                            Check first-run setup status

            Options:
              --endpoint, -e <url>             Engine endpoint (default: http://127.0.0.1:50051)
              --skip-cache                     Bypass cache for lookup commands
              --json                           Output as JSON (for piping to PowerShell)
              --help, -h                       Show this help
            """;
    }

    public static string GetCommandHelp(string command)
    {
        return command switch
        {
            "log" => """
                Usage: log <callsign> <band> <mode> [options]

                Log a new QSO. Auto-enriches with QRZ data (grid, country, etc.)

                  --station <call>     Your station callsign (if not set via setup)
                  --at <time>          QSO time (default: now). Relative (30.minutes) or absolute.
                  --rst-sent <rst>     RST sent (e.g., 59, 599)
                  --rst-rcvd <rst>     RST received
                  --freq <khz>         Frequency in kHz (e.g., 14074)
                  --no-enrich          Skip automatic QRZ lookup enrichment

                Examples:
                  log W1AW 20m FT8
                  log W1AW 40m CW --station AE7XI --rst-sent 599 --freq 7030
                  log K7ABV 20m SSB --at 30.minutes
                """,
            "get" => """
                Usage: get <local-id>

                Retrieve a QSO by its local ID (returned by the log command).
                """,
            "list" => """
                Usage: list [options]

                List QSOs with optional filters.

                  --callsign <call>    Filter by worked callsign
                  --band <band>        Filter by band (e.g., 20m)
                  --mode <mode>        Filter by mode (e.g., FT8)
                  --after <time>       QSOs after this time (e.g., 2.days, 3.hours, 2026-04-10)
                  --before <time>      QSOs before this time
                  --limit <n>          Max results (default: 20)
                  --show-id            Include the QSO local ID column
                  --show-rst           Include RST sent/received columns
                  --show-comment       Include comment/notes column
                """,
            "delete" => """
                Usage: delete <local-id>

                Delete a QSO by its local ID.
                """,
            "import" => """
                Usage: import <file-path>

                Import QSOs from an ADIF (.adi) file.
                """,
            "export" => """
                Usage: export [options]

                Export QSOs to ADIF format.

                  --file <path>        Write to file (default: stdout)
                  --include-header     Include ADIF header
                """,
            "lookup" => """
                Usage: lookup <callsign> [--skip-cache]

                Look up a callsign via QRZ.
                """,
            "stream-lookup" => """
                Usage: stream-lookup <callsign> [--skip-cache]

                Streaming lookup with progressive state updates.
                """,
            "cache-check" => """
                Usage: cache-check <callsign>

                Check if a callsign is in the engine's cache.
                """,
            "config" => """
                Usage: config [options]

                View or modify runtime configuration.

                  --set KEY=VALUE      Set a config value
                  --reset              Reset all overrides to defaults
                """,
            "setup" => """
                Usage: setup

                Check first-run setup status.
                """,
            "status" => """
                Usage: status

                Show engine sync status and QSO counts.
                """,
            _ => $"No help available for '{command}'."
        };
    }
}
