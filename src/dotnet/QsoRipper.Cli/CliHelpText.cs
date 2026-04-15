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
              sync [--force]                   Sync with QRZ logbook (--force = full sync)
              sync-status                      Detailed sync status and scheduling info
              test-logbook [--api-key <key>]   Test QRZ logbook API key
              space-weather [--refresh]        Show current NOAA space weather snapshot
              config [--set KEY=VALUE]         View or modify runtime config
              setup [--status | --from-env]    Interactive setup wizard or headless config

            Rig Control:
              rig-status                       Show rig connection and current state

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
                Usage: log <callsign> [band] [mode] [options]

                Log a new QSO. Auto-enriches with QRZ data (grid, country, etc.)

                  --from-rig           Auto-fill band, mode, and frequency from the rig
                  --band <band>        Override band (e.g., 20m)
                  --mode <mode>        Override mode (e.g., CW)
                  --station <call>     Your station callsign (if not set via setup)
                  --at <time>          QSO time (default: now). Relative (30.minutes) or absolute.
                  --rst-sent <rst>     RST sent (e.g., 59, 599)
                  --rst-rcvd <rst>     RST received
                  --freq <khz>         Frequency in kHz (e.g., 14074)
                  --comment <text>     Comment text
                  --notes <text>       Notes text
                  --no-enrich          Skip automatic QRZ lookup enrichment

                Without --from-rig, band and mode are required positional arguments.
                With --from-rig, omitted values are filled from the rig. Explicit values
                always take priority over rig-derived values.

                Examples:
                  log W1AW 20m FT8
                  log W1AW --from-rig
                  log W1AW 40m CW --from-rig --comment "Nice signal"
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

                  --at <time>          Change timestamp. Relative (30.minutes) or absolute.
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
                  update abc123 --at "2026-04-12T01:51:00Z"
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
            "rig-status" => """
                Usage: rig-status

                Show rig connection status, endpoint, and current radio state
                (frequency, band, mode) when connected.
                """,
            "sync" => """
                Usage: sync [--force]

                Trigger a sync with the QRZ logbook. Shows streaming progress updates.

                  --force              Run a full sync instead of incremental
                """,
            "sync-status" => """
                Usage: sync-status

                Show detailed sync status: local/QRZ counts, pending uploads,
                auto-sync scheduling, and last error.
                """,
            "test-logbook" => """
                Usage: test-logbook [--api-key <key>]

                Test QRZ logbook API key by querying the logbook STATUS endpoint.
                Uses the configured key if --api-key is not provided.

                  --api-key <key>      API key to test (optional)
                """,
            "space-weather" => """
                Usage: space-weather [--refresh]

                Show the current engine-backed NOAA space weather snapshot.

                  --refresh          Force an immediate refresh before printing
                  --json             Output the snapshot as JSON
                """,
            _ => $"No help available for '{command}'."
        };
    }
}
