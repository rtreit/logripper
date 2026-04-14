using Grpc.Net.Client;
using QsoRipper.Cli;
using QsoRipper.Domain;
using QsoRipper.Services;

namespace QsoRipper.Cli.Commands;

internal static class SetupCommand
{
    public static async Task<int> RunAsync(GrpcChannel channel, CliArguments arguments)
    {
        if (arguments.SetupStatus)
        {
            return await RunStatusAsync(channel, arguments.JsonOutput);
        }

        if (arguments.SetupFromEnv)
        {
            return await RunFromEnvAsync(channel);
        }

        // Default: interactive wizard
        if (Console.IsInputRedirected)
        {
            Console.Error.WriteLine("Error: stdin is redirected. Interactive wizard requires a terminal.");
            Console.Error.WriteLine("Use 'setup --from-env' for headless setup from environment variables.");
            return 1;
        }

        return await RunWizardAsync(channel);
    }

    internal static async Task<int> RunStatusAsync(GrpcChannel channel, bool jsonOutput)
    {
        var client = new SetupService.SetupServiceClient(channel);
        var response = await client.GetSetupStatusAsync(new GetSetupStatusRequest());
        var status = response.Status;

        if (status is null)
        {
            Console.WriteLine("Setup status unavailable.");
            return 1;
        }

        if (jsonOutput)
        {
            JsonOutput.Print(response);
            return status.SetupComplete ? 0 : 1;
        }

        Console.WriteLine($"Setup complete:    {status.SetupComplete}");
        Console.WriteLine($"Config path:       {status.ConfigPath}");
        Console.WriteLine($"QRZ username:      {status.QrzXmlUsername ?? "(not set)"}");
        Console.WriteLine($"QRZ Logbook key:   {(status.HasQrzLogbookApiKey ? "(configured)" : "(not set)")}");
        Console.WriteLine($"Station profile:   {status.HasStationProfile}");
#pragma warning disable CS0612 // Type or member is obsolete
        Console.WriteLine($"Storage backend:   {status.StorageBackend}");
#pragma warning restore CS0612

        return status.SetupComplete ? 0 : 1;
    }

    internal static async Task<int> RunFromEnvAsync(GrpcChannel channel)
    {
        var client = new SetupService.SetupServiceClient(channel);

        var stationCallsign = Environment.GetEnvironmentVariable("QSORIPPER_STATION_CALLSIGN");
        if (string.IsNullOrWhiteSpace(stationCallsign))
        {
            Console.Error.WriteLine("Error: QSORIPPER_STATION_CALLSIGN is required for --from-env setup.");
            return 1;
        }

        var logFilePath = Environment.GetEnvironmentVariable("QSORIPPER_LOG_FILE");
        var operatorCallsign = Environment.GetEnvironmentVariable("QSORIPPER_OPERATOR_CALLSIGN") ?? stationCallsign;
        var profileName = Environment.GetEnvironmentVariable("QSORIPPER_PROFILE_NAME") ?? "Default";
        var grid = Environment.GetEnvironmentVariable("QSORIPPER_GRID");
        var qrzUsername = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_USERNAME");
        var qrzPassword = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_PASSWORD");
        var qrzLogbookApiKey = Environment.GetEnvironmentVariable("QSORIPPER_QRZ_LOGBOOK_API_KEY");
        var autoSyncEnv = Environment.GetEnvironmentVariable("QSORIPPER_AUTO_SYNC");
        var syncIntervalEnv = Environment.GetEnvironmentVariable("QSORIPPER_SYNC_INTERVAL");

        // If no log file path, fetch the suggested one from the engine.
        if (string.IsNullOrWhiteSpace(logFilePath))
        {
            var statusResponse = await client.GetSetupStatusAsync(new GetSetupStatusRequest());
            logFilePath = statusResponse.Status?.SuggestedLogFilePath;
        }

        var profile = new StationProfile
        {
            ProfileName = profileName,
            StationCallsign = stationCallsign,
            OperatorCallsign = operatorCallsign,
        };

        if (!string.IsNullOrWhiteSpace(grid))
        {
            profile.Grid = grid;
        }

        // Validate log file step.
        var logValidation = await client.ValidateSetupStepAsync(new ValidateSetupStepRequest
        {
            Step = SetupWizardStep.LogFile,
            LogFilePath = logFilePath ?? "",
        });

        if (!logValidation.Valid)
        {
            Console.Error.WriteLine("Log file path validation failed:");
            PrintFieldErrors(logValidation.Fields);
            return 1;
        }

        // Validate station profile step.
        var profileValidation = await client.ValidateSetupStepAsync(new ValidateSetupStepRequest
        {
            Step = SetupWizardStep.StationProfiles,
            StationProfile = profile,
        });

        if (!profileValidation.Valid)
        {
            Console.Error.WriteLine("Station profile validation failed:");
            PrintFieldErrors(profileValidation.Fields);
            return 1;
        }

        // Validate QRZ step if credentials were provided.
        if (!string.IsNullOrWhiteSpace(qrzUsername) && !string.IsNullOrWhiteSpace(qrzPassword))
        {
            var qrzValidation = await client.ValidateSetupStepAsync(new ValidateSetupStepRequest
            {
                Step = SetupWizardStep.QrzIntegration,
                QrzXmlUsername = qrzUsername,
                QrzXmlPassword = qrzPassword,
            });

            if (!qrzValidation.Valid)
            {
                Console.Error.WriteLine("QRZ credentials validation failed:");
                PrintFieldErrors(qrzValidation.Fields);
                return 1;
            }
        }

        // Validate sync interval bounds if provided.
        if (!string.IsNullOrWhiteSpace(syncIntervalEnv)
            && int.TryParse(syncIntervalEnv, System.Globalization.CultureInfo.InvariantCulture, out var syncInterval)
            && syncInterval is < 60 or > 3600)
        {
            Console.Error.WriteLine("Error: QSORIPPER_SYNC_INTERVAL must be between 60 and 3600 seconds.");
            return 1;
        }

        // Save.
        var saveRequest = new SaveSetupRequest
        {
            LogFilePath = logFilePath ?? "",
            StationProfile = profile,
        };

        if (!string.IsNullOrWhiteSpace(qrzUsername))
        {
            saveRequest.QrzXmlUsername = qrzUsername;
        }

        if (!string.IsNullOrWhiteSpace(qrzPassword))
        {
            saveRequest.QrzXmlPassword = qrzPassword;
        }

        if (!string.IsNullOrWhiteSpace(qrzLogbookApiKey))
        {
            saveRequest.QrzLogbookApiKey = qrzLogbookApiKey;
        }

        // Include sync config when a logbook API key is provided.
        if (!string.IsNullOrWhiteSpace(qrzLogbookApiKey))
        {
            var autoSync = !string.IsNullOrWhiteSpace(autoSyncEnv)
                && (autoSyncEnv.Equals("true", StringComparison.OrdinalIgnoreCase)
                    || autoSyncEnv.Equals("yes", StringComparison.OrdinalIgnoreCase)
                    || autoSyncEnv == "1");

            uint intervalSeconds = 300;
            if (!string.IsNullOrWhiteSpace(syncIntervalEnv)
                && uint.TryParse(syncIntervalEnv, System.Globalization.CultureInfo.InvariantCulture, out var parsed))
            {
                intervalSeconds = parsed;
            }

            saveRequest.SyncConfig = new QsoRipper.Domain.SyncConfig
            {
                AutoSyncEnabled = autoSync,
                SyncIntervalSeconds = intervalSeconds,
            };
        }

        var saveResponse = await client.SaveSetupAsync(saveRequest);
        var savedStatus = saveResponse.Status;

        if (savedStatus is null || !savedStatus.SetupComplete)
        {
            Console.Error.WriteLine("Setup save failed. Engine did not confirm completion.");
            return 1;
        }

        Console.WriteLine("Setup complete.");
        return 0;
    }

    internal static async Task<int> RunWizardAsync(GrpcChannel channel)
    {
        var client = new SetupService.SetupServiceClient(channel);

        // Fetch initial wizard state for defaults.
        var wizardState = await client.GetSetupWizardStateAsync(new GetSetupWizardStateRequest());
        var status = wizardState.Status;

        if (status is null)
        {
            Console.Error.WriteLine("Could not retrieve setup wizard state from the engine.");
            return 1;
        }

        var isFirstRun = status.IsFirstRun;
        Console.WriteLine();
        Console.WriteLine(isFirstRun
            ? "Welcome to QsoRipper! Let's set up your station."
            : "QsoRipper Setup Wizard");
        Console.WriteLine(new string('=', 48));

        // ── Step 1: Log File Path ──────────────────────────────────
        Console.WriteLine();
        Console.WriteLine("Step 1 of 5: Log File Path");
        Console.WriteLine(new string('-', 30));

        var suggestedPath = status.SuggestedLogFilePath;
        var currentLogPath = status.LogFilePath;
        var defaultLogPath = !string.IsNullOrWhiteSpace(currentLogPath)
            ? currentLogPath
            : suggestedPath;

        if (!string.IsNullOrWhiteSpace(currentLogPath))
        {
            Console.WriteLine($"  Current: {currentLogPath}");
        }

        string logFilePath;
        while (true)
        {
            Console.Write($"Log file path [{defaultLogPath}]: ");
            var input = Console.ReadLine()?.Trim();
            logFilePath = string.IsNullOrEmpty(input) ? defaultLogPath : input;

            var validation = await client.ValidateSetupStepAsync(new ValidateSetupStepRequest
            {
                Step = SetupWizardStep.LogFile,
                LogFilePath = logFilePath,
            });

            if (validation.Valid)
            {
                break;
            }

            PrintFieldErrors(validation.Fields);
        }

        // ── Step 2: Station Profile ────────────────────────────────
        Console.WriteLine();
        Console.WriteLine("Step 2 of 5: Station Profile");
        Console.WriteLine(new string('-', 30));

        var existingProfile = status.StationProfile;
        var defaultProfileName = existingProfile?.ProfileName ?? "Default";
        var defaultStationCall = existingProfile?.StationCallsign ?? "";
        var defaultOperatorCall = existingProfile?.OperatorCallsign ?? "";
        var defaultGrid = existingProfile?.Grid ?? "";

        if (!string.IsNullOrWhiteSpace(defaultStationCall))
        {
            Console.WriteLine($"  Current station callsign: {defaultStationCall}");
        }

        string profileName;
        string stationCallsign;
        string operatorCallsign;
        string grid;

        // We collect all fields first, then validate. If invalid, re-prompt only bad fields.
        while (true)
        {
            profileName = PromptField("Profile name", defaultProfileName);
            stationCallsign = PromptField("Station callsign", defaultStationCall);
            operatorCallsign = PromptField("Operator callsign", string.IsNullOrWhiteSpace(defaultOperatorCall) ? stationCallsign : defaultOperatorCall);
            grid = PromptField("Grid square", defaultGrid);

            var profile = new StationProfile
            {
                ProfileName = profileName,
                StationCallsign = stationCallsign,
                OperatorCallsign = operatorCallsign,
            };

            if (!string.IsNullOrWhiteSpace(grid))
            {
                profile.Grid = grid;
            }

            var validation = await client.ValidateSetupStepAsync(new ValidateSetupStepRequest
            {
                Step = SetupWizardStep.StationProfiles,
                StationProfile = profile,
            });

            if (validation.Valid)
            {
                break;
            }

            PrintFieldErrors(validation.Fields);
            Console.WriteLine();

            // Update defaults with what the user just entered so they don't have to re-type valid fields.
            defaultProfileName = profileName;
            defaultStationCall = stationCallsign;
            defaultOperatorCall = operatorCallsign;
            defaultGrid = grid;
        }

        // ── Step 3: QRZ Integration (Optional) ────────────────────
        Console.WriteLine();
        Console.WriteLine("Step 3 of 5: QRZ Integration (Optional)");
        Console.WriteLine(new string('-', 30));

        string? qrzUsername = null;
        string? qrzPassword = null;

        var existingQrzUser = status.QrzXmlUsername;
        var hasExistingQrz = !string.IsNullOrWhiteSpace(existingQrzUser);

        if (hasExistingQrz)
        {
            Console.WriteLine($"  Current QRZ username: {existingQrzUser}");
        }

        var setupQrz = PromptYesNo("Set up QRZ integration?", defaultYes: false);

        if (setupQrz)
        {
            while (true)
            {
                qrzUsername = PromptField("QRZ XML username", hasExistingQrz ? existingQrzUser! : "");
                qrzPassword = PromptField("QRZ XML password", "");

                if (string.IsNullOrWhiteSpace(qrzUsername) || string.IsNullOrWhiteSpace(qrzPassword))
                {
                    Console.WriteLine("  Both username and password are required.");
                    continue;
                }

                var validation = await client.ValidateSetupStepAsync(new ValidateSetupStepRequest
                {
                    Step = SetupWizardStep.QrzIntegration,
                    QrzXmlUsername = qrzUsername,
                    QrzXmlPassword = qrzPassword,
                });

                if (!validation.Valid)
                {
                    PrintFieldErrors(validation.Fields);
                    continue;
                }

                // Offer to test credentials.
                var testConnection = PromptYesNo("Test connection?", defaultYes: true);
                if (testConnection)
                {
                    Console.Write("  Testing QRZ credentials... ");
                    var testResult = await client.TestQrzCredentialsAsync(new TestQrzCredentialsRequest
                    {
                        QrzXmlUsername = qrzUsername,
                        QrzXmlPassword = qrzPassword,
                    });

                    if (testResult.Success)
                    {
                        Console.WriteLine("OK");
                        break;
                    }

                    Console.WriteLine("FAILED");
                    Console.WriteLine($"  Error: {testResult.ErrorMessage}");

                    var retry = PromptYesNo("Retry with different credentials?", defaultYes: true);
                    if (!retry)
                    {
                        var keepAnyway = PromptYesNo("Keep these credentials anyway?", defaultYes: false);
                        if (keepAnyway)
                        {
                            break;
                        }

                        // User chose to skip QRZ entirely.
                        qrzUsername = null;
                        qrzPassword = null;
                        break;
                    }
                }
                else
                {
                    // Validation passed, user declined test — keep credentials.
                    break;
                }
            }
        }

        // ── Step 4: QRZ Logbook (Optional) ────────────────────────
        Console.WriteLine();
        Console.WriteLine("Step 4 of 5: QRZ Logbook (Optional)");
        Console.WriteLine(new string('-', 30));
        Console.WriteLine("  Enter your QRZ.com Logbook API key for bidirectional sync.");
        Console.WriteLine("  You can find this at qrz.com → Logbook → Settings → API Key.");

        string? qrzLogbookApiKey = null;
        var autoSyncEnabled = false;
        uint syncIntervalSeconds = 300;

        var hasExistingLogbookKey = status.HasQrzLogbookApiKey;
        if (hasExistingLogbookKey)
        {
            Console.WriteLine("  A logbook API key is already configured.");
        }

        var setupLogbook = PromptYesNo("Set up QRZ Logbook sync?", defaultYes: false);

        if (setupLogbook)
        {
            if (hasExistingLogbookKey)
            {
                Console.WriteLine("  Leave blank to keep the existing key.");
            }

            qrzLogbookApiKey = PromptField("QRZ Logbook API key", "");

            if (string.IsNullOrWhiteSpace(qrzLogbookApiKey))
            {
                qrzLogbookApiKey = null;
                if (!hasExistingLogbookKey)
                {
                    Console.WriteLine("  No API key entered. Skipping logbook sync.");
                }
            }

            if (!string.IsNullOrWhiteSpace(qrzLogbookApiKey) || hasExistingLogbookKey)
            {
                autoSyncEnabled = PromptYesNo("Enable automatic sync?", defaultYes: false);

                if (autoSyncEnabled)
                {
                    while (true)
                    {
                        var intervalInput = PromptField("Sync interval (seconds)", "300");
                        if (uint.TryParse(intervalInput, System.Globalization.CultureInfo.InvariantCulture, out var parsed)
                            && parsed is >= 60 and <= 3600)
                        {
                            syncIntervalSeconds = parsed;
                            break;
                        }

                        Console.WriteLine("  Sync interval must be between 60 and 3600 seconds.");
                    }
                }
            }
        }

        // ── Step 5: Review & Save ──────────────────────────────────
        Console.WriteLine();
        Console.WriteLine("Step 5 of 5: Review & Save");
        Console.WriteLine(new string('-', 30));
        Console.WriteLine();
        Console.WriteLine($"  Log file path:       {logFilePath}");
        Console.WriteLine($"  Profile name:        {profileName}");
        Console.WriteLine($"  Station callsign:    {stationCallsign}");
        Console.WriteLine($"  Operator callsign:   {operatorCallsign}");
        Console.WriteLine($"  Grid square:         {(string.IsNullOrWhiteSpace(grid) ? "(not set)" : grid)}");
        Console.WriteLine($"  QRZ username:        {(string.IsNullOrWhiteSpace(qrzUsername) ? "(not set)" : qrzUsername)}");
        Console.WriteLine($"  QRZ password:        {(string.IsNullOrWhiteSpace(qrzPassword) ? "(not set)" : "********")}");
        Console.WriteLine($"  QRZ Logbook key:     {(string.IsNullOrWhiteSpace(qrzLogbookApiKey) ? hasExistingLogbookKey ? "(configured)" : "(not set)" : "********")}");
        Console.WriteLine($"  Auto-sync:           {(autoSyncEnabled ? $"enabled (every {syncIntervalSeconds}s)" : "disabled")}");
        Console.WriteLine();

        var save = PromptYesNo("Save and apply?", defaultYes: true);
        if (!save)
        {
            Console.WriteLine("Setup cancelled.");
            return 1;
        }

        // Build save request.
        var stationProfile = new StationProfile
        {
            ProfileName = profileName,
            StationCallsign = stationCallsign,
            OperatorCallsign = operatorCallsign,
        };

        if (!string.IsNullOrWhiteSpace(grid))
        {
            stationProfile.Grid = grid;
        }

        var saveRequest = new SaveSetupRequest
        {
            LogFilePath = logFilePath,
            StationProfile = stationProfile,
        };

        if (!string.IsNullOrWhiteSpace(qrzUsername))
        {
            saveRequest.QrzXmlUsername = qrzUsername;
        }

        if (!string.IsNullOrWhiteSpace(qrzPassword))
        {
            saveRequest.QrzXmlPassword = qrzPassword;
        }

        if (!string.IsNullOrWhiteSpace(qrzLogbookApiKey))
        {
            saveRequest.QrzLogbookApiKey = qrzLogbookApiKey;
        }

        // Include sync config when the logbook feature is in use.
        if (!string.IsNullOrWhiteSpace(qrzLogbookApiKey) || hasExistingLogbookKey)
        {
            saveRequest.SyncConfig = new QsoRipper.Domain.SyncConfig
            {
                AutoSyncEnabled = autoSyncEnabled,
                SyncIntervalSeconds = syncIntervalSeconds,
            };
        }

        var saveResponse = await client.SaveSetupAsync(saveRequest);
        var savedStatus = saveResponse.Status;

        if (savedStatus is null || !savedStatus.SetupComplete)
        {
            Console.Error.WriteLine("Setup save failed. The engine did not confirm completion.");
            return 1;
        }

        Console.WriteLine();
        Console.WriteLine("Setup complete! You're ready to start logging QSOs.");
        Console.WriteLine($"Config saved to: {savedStatus.ConfigPath}");
        return 0;
    }

    internal static string PromptField(string label, string defaultValue)
    {
        if (!string.IsNullOrEmpty(defaultValue))
        {
            Console.Write($"{label} [{defaultValue}]: ");
        }
        else
        {
            Console.Write($"{label}: ");
        }

        var input = Console.ReadLine()?.Trim();
        return string.IsNullOrEmpty(input) ? defaultValue : input;
    }

    internal static bool PromptYesNo(string question, bool defaultYes)
    {
        var hint = defaultYes ? "Y/n" : "y/N";
        Console.Write($"{question} ({hint}): ");
        var input = Console.ReadLine()?.Trim();

        if (string.IsNullOrEmpty(input))
        {
            return defaultYes;
        }

        return input.Equals("y", StringComparison.OrdinalIgnoreCase)
            || input.Equals("yes", StringComparison.OrdinalIgnoreCase);
    }

    private static void PrintFieldErrors(
        Google.Protobuf.Collections.RepeatedField<SetupFieldValidation> fields)
    {
        foreach (var field in fields)
        {
            if (!field.Valid)
            {
                Console.WriteLine($"  {field.Field}: {field.Message}");
            }
        }
    }
}
