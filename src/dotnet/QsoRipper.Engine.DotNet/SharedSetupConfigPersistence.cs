using System.Text;
using System.Text.Json;
using System.Text.RegularExpressions;
using Google.Protobuf;
using QsoRipper.Domain;
using QsoRipper.EngineSelection;
using QsoRipper.Services;
using Tomlyn;
using Tomlyn.Model;

namespace QsoRipper.Engine.DotNet;

internal static class SharedSetupConfigPersistence
{
    private static readonly JsonFormatter ProtoJsonFormatter = new(JsonFormatter.Settings.Default.WithFormatDefaultValues(true));
    private static readonly JsonParser ProtoJsonParser = new(JsonParser.Settings.Default.WithIgnoreUnknownFields(true));
    private static readonly JsonSerializerOptions LegacyJsonSerializerOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
    };
    private static readonly UTF8Encoding Utf8WithoutBom = new(encoderShouldEmitUTF8Identifier: false);

    public static LoadedSharedSetupConfig Load(string configPath)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(configPath);

        var normalizedPath = Path.GetFullPath(configPath.Trim());
        if (File.Exists(normalizedPath))
        {
            return LoadFromExistingPath(normalizedPath);
        }

        foreach (var legacyCandidate in GetLegacyMigrationCandidates(normalizedPath))
        {
            if (!File.Exists(legacyCandidate.Path))
            {
                continue;
            }

            var migrated = LoadLegacyJson(legacyCandidate.Path, legacyCandidate.PreserveSensitiveValues);
            Save(normalizedPath, migrated.Config);
            return migrated;
        }

        return new LoadedSharedSetupConfig
        {
            Config = SharedPersistedSetupConfig.CreateDefault(),
        };
    }

    public static void Save(string configPath, SharedPersistedSetupConfig config)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(configPath);
        ArgumentNullException.ThrowIfNull(config);

        var normalizedPath = Path.GetFullPath(configPath.Trim());
        var directory = Path.GetDirectoryName(normalizedPath);
        if (!string.IsNullOrWhiteSpace(directory))
        {
            Directory.CreateDirectory(directory);
            if (!OperatingSystem.IsWindows())
            {
                File.SetUnixFileMode(
                    directory,
                    UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
            }
        }

        var content = Toml.FromModel(BuildModel(config));
        if (OperatingSystem.IsWindows())
        {
            File.WriteAllText(normalizedPath, content, Utf8WithoutBom);
            return;
        }

        using var stream = new FileStream(
            normalizedPath,
            new FileStreamOptions
            {
                Access = FileAccess.Write,
                Mode = FileMode.Create,
                Share = FileShare.None,
                Options = FileOptions.None,
                UnixCreateMode = UnixFileMode.UserRead | UnixFileMode.UserWrite,
            });
        using var writer = new StreamWriter(stream, Utf8WithoutBom);
        writer.Write(content);
        writer.Flush();

        File.SetUnixFileMode(normalizedPath, UnixFileMode.UserRead | UnixFileMode.UserWrite);
    }

    private static LoadedSharedSetupConfig LoadFromExistingPath(string configPath)
    {
        var content = File.ReadAllText(configPath);
        try
        {
            var model = Toml.ToModel(content);
            if (model is not TomlTable table)
            {
                if (LooksLikeJson(content))
                {
                    var migrated = LoadLegacyJsonContent(content, preserveSensitiveValues: true);
                    Save(configPath, migrated.Config);
                    return migrated;
                }

                throw new InvalidOperationException($"Config '{configPath}' did not deserialize into a TOML table.");
            }

            return new LoadedSharedSetupConfig
            {
                Config = ParseToml(table),
            };
        }
        catch (TomlException) when (LooksLikeJson(content))
        {
            var migrated = LoadLegacyJsonContent(content, preserveSensitiveValues: true);
            Save(configPath, migrated.Config);
            return migrated;
        }
        catch (Exception ex) when (ex is InvalidOperationException or FormatException or TomlException)
        {
            throw new InvalidOperationException($"Failed to parse shared setup config '{configPath}': {ex.Message}", ex);
        }
    }

    private static LoadedSharedSetupConfig LoadLegacyJson(string configPath, bool preserveSensitiveValues)
    {
        var content = File.ReadAllText(configPath);
        return LoadLegacyJsonContent(content, preserveSensitiveValues);
    }

    private static LoadedSharedSetupConfig LoadLegacyJsonContent(string content, bool preserveSensitiveValues)
    {
        var legacy = JsonSerializer.Deserialize<ManagedEnginePersistedState>(
                         content,
                         LegacyJsonSerializerOptions)
                     ?? new ManagedEnginePersistedState();

        return new LoadedSharedSetupConfig
        {
            Config = ConvertLegacyState(legacy, preserveSensitiveValues),
            LastSyncUtc = legacy.LastSyncUtc,
        };
    }

    private static IEnumerable<LegacyMigrationCandidate> GetLegacyMigrationCandidates(string configPath)
    {
        var siblingDirectory = Path.GetDirectoryName(configPath);
        if (!string.IsNullOrWhiteSpace(siblingDirectory))
        {
            yield return new LegacyMigrationCandidate(
                Path.Combine(siblingDirectory, "dotnet-engine.json"),
                PreserveSensitiveValues: true);
        }

        if (string.Equals(configPath, SharedSetupPaths.GetDefaultConfigPath(), StringComparison.OrdinalIgnoreCase))
        {
            var localAppData = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData);
            if (!string.IsNullOrWhiteSpace(localAppData))
            {
                yield return new LegacyMigrationCandidate(
                    Path.Combine(localAppData, "QsoRipper", "dotnet-engine.json"),
                    PreserveSensitiveValues: false);
            }
        }
    }

    private static SharedPersistedSetupConfig ConvertLegacyState(
        ManagedEnginePersistedState legacy,
        bool preserveSensitiveValues)
    {
        ArgumentNullException.ThrowIfNull(legacy);

        var config = SharedPersistedSetupConfig.CreateDefault();
        config.StorageBackend = "memory";
        config.QrzXmlUsername = NormalizeOptional(legacy.QrzXmlUsername);
        config.QrzXmlPassword = preserveSensitiveValues && legacy.HasQrzXmlPassword
            ? NormalizeOptional(legacy.QrzXmlPassword)
            : null;
        config.SyncConfig = ParseProtoOrDefault<SyncConfig>(legacy.SyncConfigJson, CreateDefaultSyncConfig);
        config.RigControl = ParseOptionalProto<RigControlSettings>(legacy.RigControlJson);
        config.ActiveProfileId = NormalizeOptional(legacy.ActiveProfileId);

        foreach (var entry in legacy.StationProfiles)
        {
            if (string.IsNullOrWhiteSpace(entry.ProfileId) || string.IsNullOrWhiteSpace(entry.ProfileJson))
            {
                continue;
            }

            config.StationProfiles.Add(
                new ManagedPersistedStationProfile
                {
                    ProfileId = NormalizeProfileIdOrDefault(entry.ProfileId),
                    ProfileJson = entry.ProfileJson,
                });
        }

        if (config.StationProfiles.Count == 0 && !string.IsNullOrWhiteSpace(legacy.SessionOverrideProfileJson))
        {
            var profile = ParseOptionalProto<StationProfile>(legacy.SessionOverrideProfileJson);
            if (profile is not null && StationProfileHasValues(profile))
            {
                var profileId = NormalizeProfileIdOrDefault(profile.ProfileName, profile.StationCallsign);
                config.StationProfiles.Add(
                    new ManagedPersistedStationProfile
                    {
                        ProfileId = profileId,
                        ProfileJson = ProtoJsonFormatter.Format(profile),
                    });
                config.ActiveProfileId = profileId;
            }
        }

        if (string.IsNullOrWhiteSpace(config.ActiveProfileId) && config.StationProfiles.Count > 0)
        {
            config.ActiveProfileId = config.StationProfiles[0].ProfileId;
        }

        return config;
    }

    private readonly record struct LegacyMigrationCandidate(string Path, bool PreserveSensitiveValues);

    private static SharedPersistedSetupConfig ParseToml(TomlTable root)
    {
        var config = SharedPersistedSetupConfig.CreateDefault();

        var logbook = GetTable(root, "logbook");
        var storage = GetTable(root, "storage");
        var qrzXml = GetTable(root, "qrz_xml");
        var qrzLogbook = GetTable(root, "qrz_logbook");
        var sync = GetTable(root, "sync");
        var rigControl = GetTable(root, "rig_control");
        var stationProfile = GetTable(root, "station_profile");
        var stationProfiles = GetTable(root, "station_profiles");

        config.LogbookFilePath = GetString(logbook, "file_path");
        config.StorageBackend = GetString(storage, "backend");
        config.StorageSqlitePath = GetString(storage, "sqlite_path");
        config.QrzXmlUsername = GetString(qrzXml, "username");
        config.QrzXmlPassword = GetString(qrzXml, "password");
        config.QrzXmlUserAgent = GetString(qrzXml, "user_agent");
        config.QrzLogbookApiKey = GetString(qrzLogbook, "api_key");
        config.QrzLogbookBaseUrl = GetString(qrzLogbook, "base_url");
        config.SyncConfig = ParseSyncConfig(sync);
        config.RigControl = ParseRigControl(rigControl);
        config.ActiveProfileId = NormalizeOptional(GetString(stationProfiles, "active_profile_id"));

        var entries = GetTableArray(stationProfiles, "entries");
        if (entries is not null)
        {
            foreach (var entry in entries.OfType<TomlTable>())
            {
                var profileId = NormalizeProfileIdOrDefault(GetString(entry, "profile_id"));
                var profile = ParseStationProfile(entry);
                if (!StationProfileHasValues(profile))
                {
                    continue;
                }

                config.StationProfiles.Add(
                    new ManagedPersistedStationProfile
                    {
                        ProfileId = profileId,
                        ProfileJson = ProtoJsonFormatter.Format(profile),
                    });
            }
        }

        if (config.StationProfiles.Count == 0)
        {
            var legacyProfile = ParseStationProfile(stationProfile);
            if (StationProfileHasValues(legacyProfile))
            {
                var profileId = NormalizeProfileIdOrDefault(
                    config.ActiveProfileId,
                    legacyProfile.ProfileName,
                    legacyProfile.StationCallsign);
                config.StationProfiles.Add(
                    new ManagedPersistedStationProfile
                    {
                        ProfileId = profileId,
                        ProfileJson = ProtoJsonFormatter.Format(legacyProfile),
                    });
                config.ActiveProfileId ??= profileId;
            }
        }

        if (string.IsNullOrWhiteSpace(config.ActiveProfileId) && config.StationProfiles.Count > 0)
        {
            config.ActiveProfileId = config.StationProfiles[0].ProfileId;
        }

        return config;
    }

    private static TomlTable BuildModel(SharedPersistedSetupConfig config)
    {
        var root = new TomlTable();

        var logbook = new TomlTable();
        AddIfValue(logbook, "file_path", NormalizeOptional(config.LogbookFilePath));
        AddTableIfNotEmpty(root, "logbook", logbook);

        var storage = new TomlTable();
        if (string.IsNullOrWhiteSpace(config.GetPersistedLogFilePath()) && string.Equals(config.StorageBackend, "memory", StringComparison.OrdinalIgnoreCase))
        {
            storage["backend"] = "memory";
        }

        AddTableIfNotEmpty(root, "storage", storage);

        var activeProfile = config.GetPersistedActiveProfile();
        if (activeProfile is not null && StationProfileHasValues(activeProfile))
        {
            var legacyProfile = BuildStationProfileTable(activeProfile);
            AddTableIfNotEmpty(root, "station_profile", legacyProfile);
        }

        if (config.StationProfiles.Count > 0 || !string.IsNullOrWhiteSpace(config.ActiveProfileId))
        {
            var stationProfiles = new TomlTable();
            AddIfValue(stationProfiles, "active_profile_id", NormalizeOptional(config.ActiveProfileId));

            if (config.StationProfiles.Count > 0)
            {
                var entries = new TomlTableArray();
                foreach (var entry in config.StationProfiles)
                {
                    if (string.IsNullOrWhiteSpace(entry.ProfileId) || string.IsNullOrWhiteSpace(entry.ProfileJson))
                    {
                        continue;
                    }

                    var profile = ParseProtoOrDefault<StationProfile>(entry.ProfileJson, static () => new StationProfile());
                    if (!StationProfileHasValues(profile))
                    {
                        continue;
                    }

                    var table = BuildStationProfileTable(profile);
                    table["profile_id"] = entry.ProfileId;
                    entries.Add(table);
                }

                if (entries.Count > 0)
                {
                    stationProfiles["entries"] = entries;
                }
            }

            AddTableIfNotEmpty(root, "station_profiles", stationProfiles);
        }

        var qrzXml = new TomlTable();
        AddIfValue(qrzXml, "username", NormalizeOptional(config.QrzXmlUsername));
        AddIfValue(qrzXml, "password", NormalizeOptional(config.QrzXmlPassword));
        AddIfValue(qrzXml, "user_agent", NormalizeOptional(config.QrzXmlUserAgent));
        AddTableIfNotEmpty(root, "qrz_xml", qrzXml);

        var qrzLogbook = new TomlTable();
        AddIfValue(qrzLogbook, "api_key", NormalizeOptional(config.QrzLogbookApiKey));
        AddIfValue(qrzLogbook, "base_url", NormalizeOptional(config.QrzLogbookBaseUrl));
        AddTableIfNotEmpty(root, "qrz_logbook", qrzLogbook);

        var sync = BuildSyncTable(config.SyncConfig);
        AddTableIfNotEmpty(root, "sync", sync);

        var rigControl = BuildRigControlTable(config.RigControl);
        AddTableIfNotEmpty(root, "rig_control", rigControl);

        return root;
    }

    private static SyncConfig ParseSyncConfig(TomlTable? table)
    {
        var config = CreateDefaultSyncConfig();
        if (table is null)
        {
            return config;
        }

        config.AutoSyncEnabled = GetBoolean(table, "auto_sync_enabled") ?? false;
        config.SyncIntervalSeconds = GetUInt32(table, "sync_interval_seconds") ?? 300u;
        config.ConflictPolicy = GetString(table, "conflict_policy") switch
        {
            "flag_for_review" => ConflictPolicy.FlagForReview,
            _ => ConflictPolicy.LastWriteWins,
        };
        return config;
    }

    private static TomlTable BuildSyncTable(SyncConfig config)
    {
        var table = new TomlTable();
        if (config.AutoSyncEnabled)
        {
            table["auto_sync_enabled"] = true;
        }

        var interval = config.SyncIntervalSeconds == 0 ? 300u : config.SyncIntervalSeconds;
        if (interval != 300u)
        {
            table["sync_interval_seconds"] = interval;
        }

        if (config.ConflictPolicy == ConflictPolicy.FlagForReview)
        {
            table["conflict_policy"] = "flag_for_review";
        }

        return table;
    }

    private static RigControlSettings? ParseRigControl(TomlTable? table)
    {
        if (table is null)
        {
            return null;
        }

        var settings = new RigControlSettings();
        var enabled = GetBoolean(table, "enabled");
        if (enabled is not null)
        {
            settings.Enabled = enabled.Value;
        }

        var host = GetString(table, "host");
        if (!string.IsNullOrWhiteSpace(host))
        {
            settings.Host = host;
        }

        var port = GetUInt32(table, "port");
        if (port is not null)
        {
            settings.Port = port.Value;
        }

        var readTimeoutMs = GetUInt64(table, "read_timeout_ms");
        if (readTimeoutMs is not null)
        {
            settings.ReadTimeoutMs = readTimeoutMs.Value;
        }

        var staleThresholdMs = GetUInt64(table, "stale_threshold_ms");
        if (staleThresholdMs is not null)
        {
            settings.StaleThresholdMs = staleThresholdMs.Value;
        }

        return RigControlHasValues(settings) ? settings : null;
    }

    private static TomlTable BuildRigControlTable(RigControlSettings? settings)
    {
        var table = new TomlTable();
        if (settings is null)
        {
            return table;
        }

        if (settings.HasEnabled)
        {
            table["enabled"] = settings.Enabled;
        }

        if (settings.HasHost)
        {
            AddIfValue(table, "host", NormalizeOptional(settings.Host));
        }

        if (settings.HasPort)
        {
            table["port"] = settings.Port;
        }

        if (settings.HasReadTimeoutMs)
        {
            table["read_timeout_ms"] = settings.ReadTimeoutMs;
        }

        if (settings.HasStaleThresholdMs)
        {
            table["stale_threshold_ms"] = settings.StaleThresholdMs;
        }

        return table;
    }

    private static StationProfile ParseStationProfile(TomlTable? table)
    {
        if (table is null)
        {
            return new StationProfile();
        }

        var profile = new StationProfile
        {
            StationCallsign = GetString(table, "station_callsign") ?? string.Empty,
        };

        var profileName = GetString(table, "profile_name");
        if (!string.IsNullOrWhiteSpace(profileName))
        {
            profile.ProfileName = profileName;
        }

        var operatorCallsign = GetString(table, "operator_callsign");
        if (!string.IsNullOrWhiteSpace(operatorCallsign))
        {
            profile.OperatorCallsign = operatorCallsign;
        }

        var operatorName = GetString(table, "operator_name");
        if (!string.IsNullOrWhiteSpace(operatorName))
        {
            profile.OperatorName = operatorName;
        }

        var grid = GetString(table, "grid");
        if (!string.IsNullOrWhiteSpace(grid))
        {
            profile.Grid = grid;
        }

        var county = GetString(table, "county");
        if (!string.IsNullOrWhiteSpace(county))
        {
            profile.County = county;
        }

        var state = GetString(table, "state");
        if (!string.IsNullOrWhiteSpace(state))
        {
            profile.State = state;
        }

        var country = GetString(table, "country");
        if (!string.IsNullOrWhiteSpace(country))
        {
            profile.Country = country;
        }

        var arrlSection = GetString(table, "arrl_section");
        if (!string.IsNullOrWhiteSpace(arrlSection))
        {
            profile.ArrlSection = arrlSection;
        }

        var dxcc = GetUInt32(table, "dxcc");
        if (dxcc is not null)
        {
            profile.Dxcc = dxcc.Value;
        }

        var cqZone = GetUInt32(table, "cq_zone");
        if (cqZone is not null)
        {
            profile.CqZone = cqZone.Value;
        }

        var ituZone = GetUInt32(table, "itu_zone");
        if (ituZone is not null)
        {
            profile.ItuZone = ituZone.Value;
        }

        var latitude = GetDouble(table, "latitude");
        if (latitude is not null)
        {
            profile.Latitude = latitude.Value;
        }

        var longitude = GetDouble(table, "longitude");
        if (longitude is not null)
        {
            profile.Longitude = longitude.Value;
        }

        return profile;
    }

    private static TomlTable BuildStationProfileTable(StationProfile profile)
    {
        var table = new TomlTable();
        AddIfValue(table, "profile_name", NormalizeOptional(profile.ProfileName));
        AddIfValue(table, "station_callsign", NormalizeOptional(profile.StationCallsign));
        AddIfValue(table, "operator_callsign", NormalizeOptional(profile.OperatorCallsign));
        AddIfValue(table, "operator_name", NormalizeOptional(profile.OperatorName));
        AddIfValue(table, "grid", NormalizeOptional(profile.Grid));
        AddIfValue(table, "county", NormalizeOptional(profile.County));
        AddIfValue(table, "state", NormalizeOptional(profile.State));
        AddIfValue(table, "country", NormalizeOptional(profile.Country));
        AddIfValue(table, "arrl_section", NormalizeOptional(profile.ArrlSection));
        if (profile.HasDxcc)
        {
            table["dxcc"] = profile.Dxcc;
        }

        if (profile.HasCqZone)
        {
            table["cq_zone"] = profile.CqZone;
        }

        if (profile.HasItuZone)
        {
            table["itu_zone"] = profile.ItuZone;
        }

        if (profile.HasLatitude)
        {
            table["latitude"] = profile.Latitude;
        }

        if (profile.HasLongitude)
        {
            table["longitude"] = profile.Longitude;
        }

        return table;
    }

    private static T ParseProtoOrDefault<T>(string? json, Func<T> defaultFactory)
        where T : class, IMessage<T>, new()
    {
        return string.IsNullOrWhiteSpace(json) ? defaultFactory() : ProtoJsonParser.Parse<T>(json);
    }

    private static T? ParseOptionalProto<T>(string? json)
        where T : class, IMessage<T>, new()
    {
        return string.IsNullOrWhiteSpace(json) ? null : ProtoJsonParser.Parse<T>(json);
    }

    private static SyncConfig CreateDefaultSyncConfig()
    {
        return new SyncConfig
        {
            AutoSyncEnabled = false,
            SyncIntervalSeconds = 300,
            ConflictPolicy = ConflictPolicy.LastWriteWins,
        };
    }

    private static bool StationProfileHasValues(StationProfile profile)
    {
        return !string.IsNullOrWhiteSpace(profile.ProfileName)
            || !string.IsNullOrWhiteSpace(profile.StationCallsign)
            || !string.IsNullOrWhiteSpace(profile.OperatorCallsign)
            || !string.IsNullOrWhiteSpace(profile.OperatorName)
            || !string.IsNullOrWhiteSpace(profile.Grid)
            || !string.IsNullOrWhiteSpace(profile.County)
            || !string.IsNullOrWhiteSpace(profile.State)
            || !string.IsNullOrWhiteSpace(profile.Country)
            || !string.IsNullOrWhiteSpace(profile.ArrlSection)
            || profile.HasDxcc
            || profile.HasCqZone
            || profile.HasItuZone
            || profile.HasLatitude
            || profile.HasLongitude;
    }

    private static bool RigControlHasValues(RigControlSettings settings)
    {
        return settings.HasEnabled
            || settings.HasHost
            || settings.HasPort
            || settings.HasReadTimeoutMs
            || settings.HasStaleThresholdMs;
    }

    private static bool LooksLikeJson(string content)
    {
        foreach (var character in content)
        {
            if (char.IsWhiteSpace(character))
            {
                continue;
            }

            return character is '{' or '[';
        }

        return false;
    }

    private static void AddIfValue(TomlTable table, string key, string? value)
    {
        if (!string.IsNullOrWhiteSpace(value))
        {
            table[key] = value;
        }
    }

    private static void AddIfValue(TomlTable table, string key, bool? value)
    {
        if (value is not null)
        {
            table[key] = value.Value;
        }
    }

    private static void AddIfValue(TomlTable table, string key, uint? value)
    {
        if (value is not null)
        {
            table[key] = value.Value;
        }
    }

    private static void AddIfValue(TomlTable table, string key, ulong? value)
    {
        if (value is not null)
        {
            table[key] = value.Value;
        }
    }

    private static void AddIfValue(TomlTable table, string key, double? value)
    {
        if (value is not null)
        {
            table[key] = value.Value;
        }
    }

    private static void AddTableIfNotEmpty(TomlTable root, string key, TomlTable table)
    {
        if (table.Count > 0)
        {
            root[key] = table;
        }
    }

    private static TomlTable? GetTable(TomlTable? table, string key)
    {
        if (table is null)
        {
            return null;
        }

        return table.TryGetValue(key, out var value) ? value as TomlTable : null;
    }

    private static TomlTableArray? GetTableArray(TomlTable? table, string key)
    {
        if (table is null)
        {
            return null;
        }

        return table.TryGetValue(key, out var value) ? value as TomlTableArray : null;
    }

    private static string? GetString(TomlTable? table, string key)
    {
        if (table is null || !table.TryGetValue(key, out var value))
        {
            return null;
        }

        return NormalizeOptional(value as string);
    }

    private static bool? GetBoolean(TomlTable? table, string key)
    {
        if (table is null || !table.TryGetValue(key, out var value))
        {
            return null;
        }

        return value switch
        {
            bool booleanValue => booleanValue,
            _ => null,
        };
    }

    private static uint? GetUInt32(TomlTable? table, string key)
    {
        if (table is null || !table.TryGetValue(key, out var value))
        {
            return null;
        }

        return value switch
        {
            long longValue when longValue >= 0 && longValue <= uint.MaxValue => (uint)longValue,
            int intValue when intValue >= 0 => (uint)intValue,
            uint uintValue => uintValue,
            _ => null,
        };
    }

    private static ulong? GetUInt64(TomlTable? table, string key)
    {
        if (table is null || !table.TryGetValue(key, out var value))
        {
            return null;
        }

        return value switch
        {
            long longValue when longValue >= 0 => (ulong)longValue,
            int intValue when intValue >= 0 => (ulong)intValue,
            uint uintValue => uintValue,
            ulong ulongValue => ulongValue,
            _ => null,
        };
    }

    private static double? GetDouble(TomlTable? table, string key)
    {
        if (table is null || !table.TryGetValue(key, out var value))
        {
            return null;
        }

        return value switch
        {
            double doubleValue => doubleValue,
            float floatValue => floatValue,
            long longValue => longValue,
            int intValue => intValue,
            _ => null,
        };
    }

    private static string? NormalizeOptional(string? value)
    {
        return string.IsNullOrWhiteSpace(value) ? null : value.Trim();
    }

    private static string NormalizeProfileIdOrDefault(params string?[] candidates)
    {
        foreach (var candidate in candidates)
        {
            if (string.IsNullOrWhiteSpace(candidate))
            {
                continue;
            }

#pragma warning disable CA1308 // Profile IDs intentionally match Rust's lowercase normalization.
            var normalized = Regex.Replace(candidate.Trim().ToLowerInvariant(), "[^a-z0-9]+", "-").Trim('-');
#pragma warning restore CA1308
            if (!string.IsNullOrWhiteSpace(normalized))
            {
                return normalized;
            }
        }

        return "default";
    }
}

internal sealed class LoadedSharedSetupConfig
{
    public required SharedPersistedSetupConfig Config { get; init; }

    public DateTimeOffset? LastSyncUtc { get; init; }
}

internal sealed class SharedPersistedSetupConfig
{
    public string? LogbookFilePath { get; set; }

    public string? StorageBackend { get; set; }

    public string? StorageSqlitePath { get; set; }

    public string? ActiveProfileId { get; set; }

    public List<ManagedPersistedStationProfile> StationProfiles { get; } = [];

    public string? QrzXmlUsername { get; set; }

    public string? QrzXmlPassword { get; set; }

    public string? QrzXmlUserAgent { get; set; }

    public string? QrzLogbookApiKey { get; set; }

    public string? QrzLogbookBaseUrl { get; set; }

    public SyncConfig SyncConfig { get; set; } = new();

    public RigControlSettings? RigControl { get; set; }

    public static SharedPersistedSetupConfig CreateDefault()
    {
        return new SharedPersistedSetupConfig
        {
            SyncConfig = new SyncConfig
            {
                AutoSyncEnabled = false,
                SyncIntervalSeconds = 300,
                ConflictPolicy = ConflictPolicy.LastWriteWins,
            }
        };
    }

    public string? GetPersistedLogFilePath()
    {
        return string.IsNullOrWhiteSpace(LogbookFilePath)
            ? (string.IsNullOrWhiteSpace(StorageSqlitePath) ? null : StorageSqlitePath.Trim())
            : LogbookFilePath.Trim();
    }

    public StationProfile? GetPersistedActiveProfile()
    {
        var profileJson = StationProfiles
            .FirstOrDefault(entry => string.Equals(entry.ProfileId, ActiveProfileId, StringComparison.Ordinal))
            ?.ProfileJson
            ?? StationProfiles.FirstOrDefault()?.ProfileJson;

        return string.IsNullOrWhiteSpace(profileJson)
            ? null
            : new JsonParser(JsonParser.Settings.Default.WithIgnoreUnknownFields(true)).Parse<StationProfile>(profileJson);
    }
}
