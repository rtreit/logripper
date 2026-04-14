namespace QsoRipper.Cli;

internal static class CliHelpText
{
    public static string GetGeneralHelp()
    {
        return """
            QsoRipper CLI

            Usage: qsoripper-cli [options] <command> [arguments]

            Logbook:
              log <call> <band> <mode>         Log a QSO (e.g., log W1AW 20m FT8)
              get <local-id>                   Get a QSO by ID
              list [filters]                   List QSOs (--callsign, --band, --mode, --limit)
              update <local-id> [fields]       Update a QSO (--grid, --freq, --enrich, etc.)
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
              setup [--status | --from-env]    Interactive setup wizard or headless config

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
                  --comment <text>     Comment text
                  --notes <text>       Notes text
                  --no-enrich          Skip automatic QRZ lookup enrichment

                Examples:
                  log W1AW 20m FT8
                  log W1AW 40m CW --station AE7XI --rst-sent 599 --freq 7030 --comment "Nice signal"
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
                  --show-comment       Include comment/notes column (default)
                """,
            "update" => """
                Usage: update <local-id> [options]

                Update fields on an existing QSO.

                  --grid <grid>        Set grid square (e.g., CN87)
                  --country <name>     Set country
                  --state <abbr>       Set state (e.g., WA)
                  --freq <khz>         Set frequency in kHz
                  --band <band>        Change band
                  --mode <mode>        Change mode
                  --rst-sent <rst>     Update RST sent
                  --rst-rcvd <rst>     Update RST received
                  --comment <text>     Set comment/notes
                  --enrich             Re-run QRZ lookup enrichment

                Examples:
                  update abc123 --grid CN87 --freq 14074
                  update abc123 --enrich
                """,
            "delete" => """
                Usage: delete <local-id>

                Delete a QSO by its local ID.
                """,
            "import" => """
                Usage: import <file-path> [--refresh]

                Import QSOs from an ADIF (.adi) file.

                  --refresh            Update existing records instead of skipping duplicates.
                                       Non-empty import fields overwrite; fields absent from the
                                       import are preserved.
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
                Usage: setup [--status] [--from-env]

                Interactive setup wizard for first-run configuration. Walks through
                log file path, station profile, and optional QRZ integration step by step.

                Modes:
                  (default)          Interactive wizard — prompts for each setting
                  --status           Show current setup status (read-only)
                  --from-env         Headless setup from environment variables (no prompts)

                Environment variables for --from-env:
                  QSORIPPER_LOG_FILE              Log file path
                  QSORIPPER_STATION_CALLSIGN      Station callsign (required)
                  QSORIPPER_OPERATOR_CALLSIGN     Operator callsign (defaults to station)
                  QSORIPPER_PROFILE_NAME          Profile name (defaults to "Default")
                  QSORIPPER_GRID                  Grid square
                  QSORIPPER_QRZ_USERNAME          QRZ XML username (optional)
                  QSORIPPER_QRZ_PASSWORD          QRZ XML password (optional)

                Examples:
                  setup                           Start the interactive wizard
                  setup --status                  Check if setup is complete
                  setup --from-env                Configure from environment variables
                """,
            "status" => """
                Usage: status

                Show engine sync status and QSO counts.
                """,
            _ => $"No help available for '{command}'."
        };
    }
}
