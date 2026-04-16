using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Text.Json;
using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;
using QsoRipper.Gui.Utilities;
using QsoRipper.Services;

namespace QsoRipper.Gui.Inspection;

internal sealed record UxCaptureFixture
{
    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        PropertyNameCaseInsensitive = true,
        WriteIndented = true
    };

    public string ActiveLogFilePath { get; init; } = @"C:\logs\sample-log.adi";

    public string ProfileName { get; init; } = "Portable";

    public string StationCallsign { get; init; } = "K7RND";

    public string OperatorCallsign { get; init; } = "K7RND";

    public string OperatorName { get; init; } = "Randy";

    public string GridSquare { get; init; } = "CN87";

    public string County { get; init; } = string.Empty;

    public string State { get; init; } = "WA";

    public string Country { get; init; } = "United States";

    public string ArrlSection { get; init; } = "WWA";

    public uint Dxcc { get; init; } = 291;

    public uint CqZone { get; init; } = 3;

    public uint ItuZone { get; init; } = 6;

    public double Latitude { get; init; }

    public double Longitude { get; init; }

    public string SearchText { get; init; } = string.Empty;

    public string? SelectedLocalId { get; init; }

    public bool ConfigFileExists { get; init; } = true;

    public bool SetupComplete { get; init; } = true;

    public bool IsFirstRun { get; init; }

    public string ConfigPath { get; init; } = @"C:\Users\Public\QsoRipper\config.toml";

    public string? QrzXmlUsername { get; init; }

    public bool HasQrzXmlPassword { get; init; }

    public bool HasQrzLogbookApiKey { get; init; }

    public bool PersistenceStepEnabled { get; init; } = true;

    public string PersistenceLabel { get; init; } = "Storage";

    public string PersistenceDescription { get; init; } = "Choose where the fixture engine stores its logbook data.";

    public DateTimeOffset? LastSyncUtc { get; init; } = new(2026, 4, 13, 22, 30, 0, TimeSpan.Zero);

    public bool AutoSyncEnabled { get; init; } = true;

    public int SyncIntervalSeconds { get; init; } = 300;

    public string ConflictPolicy { get; init; } = nameof(QsoRipper.Domain.ConflictPolicy.LastWriteWins);

    public bool? RigControlEnabled { get; init; }

    public string? RigControlHost { get; init; }

    public uint? RigControlPort { get; init; }

    public ulong? RigControlReadTimeoutMs { get; init; }

    public ulong? RigControlStaleThresholdMs { get; init; }

    public bool IsSyncing { get; init; }

    public IReadOnlyList<UxCaptureQsoFixtureItem> RecentQsos { get; init; } = CreateDefaultRecentQsos();

    public static UxCaptureFixture Load(string? fixturePath)
    {
        if (string.IsNullOrWhiteSpace(fixturePath))
        {
            return new UxCaptureFixture();
        }

        if (!File.Exists(fixturePath))
        {
            throw new FileNotFoundException("Capture fixture was not found.", fixturePath);
        }

        using var stream = File.OpenRead(fixturePath);
        var fixture = JsonSerializer.Deserialize<UxCaptureFixture>(stream, JsonOptions);
        if (fixture is null)
        {
            throw new InvalidOperationException($"Capture fixture '{fixturePath}' could not be parsed.");
        }

        return fixture with
        {
            RecentQsos = fixture.RecentQsos.Count == 0
                ? CreateDefaultRecentQsos()
                : fixture.RecentQsos
        };
    }

    public IReadOnlyList<QsoRecord> BuildRecentQsoRecords()
        => RecentQsos.Select(MapToQsoRecord).ToArray();

    public StationProfile BuildStationProfile()
    {
        var profile = new StationProfile
        {
            ProfileName = ProfileName,
            StationCallsign = StationCallsign,
            OperatorCallsign = OperatorCallsign,
            OperatorName = OperatorName,
            Grid = GridSquare,
            County = County,
            State = State,
            Country = Country,
            ArrlSection = ArrlSection,
            Dxcc = Dxcc,
            CqZone = CqZone,
            ItuZone = ItuZone,
        };

        if (Latitude != 0)
        {
            profile.Latitude = Latitude;
        }

        if (Longitude != 0)
        {
            profile.Longitude = Longitude;
        }

        return profile;
    }

    public RigControlSettings? BuildRigControlSettings()
    {
        var hasValues = RigControlEnabled.HasValue
            || !string.IsNullOrWhiteSpace(RigControlHost)
            || RigControlPort.HasValue
            || RigControlReadTimeoutMs.HasValue
            || RigControlStaleThresholdMs.HasValue;

        if (!hasValues)
        {
            return null;
        }

        var settings = new RigControlSettings();
        if (RigControlEnabled.HasValue)
        {
            settings.Enabled = RigControlEnabled.Value;
        }

        if (!string.IsNullOrWhiteSpace(RigControlHost))
        {
            settings.Host = RigControlHost;
        }

        if (RigControlPort.HasValue)
        {
            settings.Port = RigControlPort.Value;
        }

        if (RigControlReadTimeoutMs.HasValue)
        {
            settings.ReadTimeoutMs = RigControlReadTimeoutMs.Value;
        }

        if (RigControlStaleThresholdMs.HasValue)
        {
            settings.StaleThresholdMs = RigControlStaleThresholdMs.Value;
        }

        return settings;
    }

    public SyncConfig BuildSyncConfig() => new()
    {
        AutoSyncEnabled = AutoSyncEnabled,
        SyncIntervalSeconds = SyncIntervalSeconds > 0 ? (uint)SyncIntervalSeconds : 300,
        ConflictPolicy = ParseConflictPolicy(ConflictPolicy)
    };

    private static QsoRipper.Domain.ConflictPolicy ParseConflictPolicy(string? value)
        => System.Enum.TryParse<QsoRipper.Domain.ConflictPolicy>(value, ignoreCase: true, out var policy)
            ? policy
            : QsoRipper.Domain.ConflictPolicy.LastWriteWins;

    private static IReadOnlyList<UxCaptureQsoFixtureItem> CreateDefaultRecentQsos() =>
    [
        new()
        {
            LocalId = "qso-1",
            WorkedCallsign = "W1AW",
            UtcTimestamp = new DateTimeOffset(2026, 4, 13, 22, 16, 0, TimeSpan.Zero),
            Band = "40M",
            Mode = "CW",
            FrequencyKhz = 7025,
            WorkedGrid = "FN31",
            Comment = "Evening CW run with strong signal reports and a narrow note field.",
            WorkedOperatorName = "ARRL HQ",
            WorkedCountry = "United States",
            WorkedState = "CT",
            WorkedDxcc = 291,
            ContestId = "ARRL-DX",
            ExchangeReceived = "CT",
            RstSent = "599",
            RstReceived = "579",
            SyncStatus = "LocalOnly"
        },
        new()
        {
            LocalId = "qso-2",
            WorkedCallsign = "K7RND",
            UtcTimestamp = new DateTimeOffset(2026, 4, 13, 22, 15, 0, TimeSpan.Zero),
            Band = "20M",
            Mode = "FT8",
            FrequencyKhz = 14074,
            WorkedGrid = "CN87",
            Comment = "Portable park activation exchange with a longer operator note to exercise truncation.",
            WorkedOperatorName = "Randy",
            WorkedCountry = "United States",
            WorkedState = "WA",
            WorkedDxcc = 291,
            ContestId = "POTA",
            ExchangeReceived = "K-1234",
            RstSent = "59",
            RstReceived = "59",
            SyncStatus = "Synced"
        },
        new()
        {
            LocalId = "qso-3",
            WorkedCallsign = "VE7ABC",
            UtcTimestamp = new DateTimeOffset(2026, 4, 13, 22, 12, 0, TimeSpan.Zero),
            Band = "15M",
            Mode = "SSB",
            FrequencyKhz = 21295,
            WorkedGrid = "CN89",
            Comment = "Late-afternoon SSB contact with a country column long enough to ellipsize cleanly.",
            WorkedOperatorName = "Jordan",
            WorkedCountry = "Canada",
            WorkedState = "BC",
            WorkedDxcc = 1,
            ContestId = "CQ-WW",
            ExchangeReceived = "BC",
            RstSent = "59",
            RstReceived = "57",
            SyncStatus = "Modified"
        },
        new()
        {
            LocalId = "qso-4",
            WorkedCallsign = "DL1XYZ",
            UtcTimestamp = new DateTimeOffset(2026, 4, 13, 22, 8, 0, TimeSpan.Zero),
            Band = "10M",
            Mode = "SSB",
            FrequencyKhz = 28485,
            WorkedGrid = "JO62",
            Comment = "DX opening note for visual density capture.",
            WorkedOperatorName = "Marta",
            WorkedCountry = "Federal Republic of Germany",
            WorkedState = string.Empty,
            WorkedDxcc = 230,
            ContestId = "DX",
            ExchangeReceived = "59",
            RstSent = "59",
            RstReceived = "55",
            SyncStatus = "Conflict"
        }
    ];

    private static QsoRecord MapToQsoRecord(UxCaptureQsoFixtureItem item)
    {
        if (!ProtoEnumDisplay.TryParseBand(item.Band, out var band))
        {
            throw new InvalidDataException($"Unsupported band '{item.Band}' in capture fixture.");
        }

        if (!ProtoEnumDisplay.TryParseMode(item.Mode, out var mode))
        {
            throw new InvalidDataException($"Unsupported mode '{item.Mode}' in capture fixture.");
        }

        if (!System.Enum.TryParse<SyncStatus>(item.SyncStatus, ignoreCase: true, out var syncStatus))
        {
            throw new InvalidDataException($"Unsupported sync status '{item.SyncStatus}' in capture fixture.");
        }

        return new QsoRecord
        {
            LocalId = item.LocalId,
            WorkedCallsign = item.WorkedCallsign,
            StationCallsign = string.IsNullOrWhiteSpace(item.StationCallsign) ? "K7RND" : item.StationCallsign,
            UtcTimestamp = Timestamp.FromDateTimeOffset(item.UtcTimestamp),
            Band = band,
            Mode = mode,
            FrequencyKhz = item.FrequencyKhz,
            WorkedGrid = item.WorkedGrid ?? string.Empty,
            Comment = item.Comment ?? string.Empty,
            Notes = item.Notes ?? string.Empty,
            WorkedOperatorName = item.WorkedOperatorName ?? string.Empty,
            WorkedState = item.WorkedState ?? string.Empty,
            WorkedCountry = item.WorkedCountry ?? string.Empty,
            WorkedDxcc = item.WorkedDxcc,
            ContestId = item.ContestId ?? string.Empty,
            ExchangeReceived = item.ExchangeReceived ?? string.Empty,
            RstSent = new RstReport { Raw = item.RstSent ?? string.Empty },
            RstReceived = new RstReport { Raw = item.RstReceived ?? string.Empty },
            SyncStatus = syncStatus
        };
    }
}

internal sealed record UxCaptureQsoFixtureItem
{
    public string LocalId { get; init; } = Guid.NewGuid().ToString("N");

    public string WorkedCallsign { get; init; } = "W1AW";

    public string StationCallsign { get; init; } = "K7RND";

    public DateTimeOffset UtcTimestamp { get; init; } = new(2026, 4, 13, 22, 15, 0, TimeSpan.Zero);

    public string Band { get; init; } = "20M";

    public string Mode { get; init; } = "FT8";

    public ulong FrequencyKhz { get; init; } = 14074;

    public string? WorkedGrid { get; init; }

    public string? Comment { get; init; }

    public string? Notes { get; init; }

    public string? WorkedOperatorName { get; init; }

    public string? WorkedState { get; init; }

    public string? WorkedCountry { get; init; }

    public uint WorkedDxcc { get; init; }

    public string? ContestId { get; init; }

    public string? ExchangeReceived { get; init; }

    public string? RstSent { get; init; } = "59";

    public string? RstReceived { get; init; } = "59";

    public string SyncStatus { get; init; } = nameof(QsoRipper.Domain.SyncStatus.LocalOnly);
}
