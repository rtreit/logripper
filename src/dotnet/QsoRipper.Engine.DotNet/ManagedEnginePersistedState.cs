using System.Collections.ObjectModel;
using System.Text.Json.Serialization;

namespace QsoRipper.Engine.DotNet;

internal sealed class ManagedEnginePersistedState
{
    public string? QrzXmlUsername { get; set; }

    public string? QrzXmlPassword { get; set; }

    public bool HasQrzXmlPassword { get; set; }

    public bool HasQrzLogbookApiKey { get; set; }

    public string? SyncConfigJson { get; set; }

    public string? RigControlJson { get; set; }

    public Collection<ManagedPersistedStationProfile> StationProfiles { get; init; } = [];

    public string? ActiveProfileId { get; set; }

    public string? SessionOverrideProfileJson { get; set; }

    public DateTimeOffset? LastSyncUtc { get; set; }

    public Dictionary<string, string> RuntimeOverrides { get; init; } = new(StringComparer.OrdinalIgnoreCase);
}

internal sealed class ManagedPersistedStationProfile
{
    public string ProfileId { get; set; } = string.Empty;

    public string ProfileJson { get; set; } = string.Empty;
}
