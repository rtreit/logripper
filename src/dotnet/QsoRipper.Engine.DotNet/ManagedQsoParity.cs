using Google.Protobuf.WellKnownTypes;
using QsoRipper.Domain;

namespace QsoRipper.Engine.DotNet;

internal static class ManagedQsoParity
{
    public static StationSnapshot? StationSnapshotFromProfile(StationProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        var snapshot = new StationSnapshot
        {
            StationCallsign = NormalizeRequiredString(profile.StationCallsign),
            ProfileName = NormalizeOptionalString(profile.ProfileName),
            OperatorCallsign = NormalizeOptionalString(profile.OperatorCallsign),
            OperatorName = NormalizeOptionalString(profile.OperatorName),
            Grid = NormalizeOptionalString(profile.Grid),
            County = NormalizeOptionalString(profile.County),
            State = NormalizeOptionalString(profile.State),
            Country = NormalizeOptionalString(profile.Country),
            ArrlSection = NormalizeOptionalString(profile.ArrlSection),
        };

        if (profile.Dxcc > 0)
        {
            snapshot.Dxcc = profile.Dxcc;
        }

        if (profile.CqZone > 0)
        {
            snapshot.CqZone = profile.CqZone;
        }

        if (profile.ItuZone > 0)
        {
            snapshot.ItuZone = profile.ItuZone;
        }

        if (profile.Latitude != 0)
        {
            snapshot.Latitude = profile.Latitude;
        }

        if (profile.Longitude != 0)
        {
            snapshot.Longitude = profile.Longitude;
        }

        NormalizeStationSnapshot(snapshot);
        return StationSnapshotHasValues(snapshot) ? snapshot : null;
    }

    public static StationSnapshot? EffectiveStationSnapshot(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        var snapshot = qso.StationSnapshot?.Clone() ?? new StationSnapshot();
        var stationCallsign = NormalizeRequiredString(qso.StationCallsign);
        if (!string.IsNullOrEmpty(stationCallsign))
        {
            snapshot.StationCallsign = stationCallsign;
        }

        NormalizeStationSnapshot(snapshot);
        return StationSnapshotHasValues(snapshot) ? snapshot : null;
    }

    public static void MaterializeStationSnapshotForCreate(QsoRecord qso, StationProfile? activeProfile)
    {
        ArgumentNullException.ThrowIfNull(qso);

        var snapshot = activeProfile is null
            ? new StationSnapshot()
            : StationSnapshotFromProfile(activeProfile)?.Clone() ?? new StationSnapshot();

        if (qso.StationSnapshot is not null)
        {
            MergeStationSnapshot(snapshot, qso.StationSnapshot, clearBlankStrings: false);
        }

        var effectiveCallsign = TrimmedNonEmpty(qso.StationCallsign)
            ?? (qso.StationSnapshot is not null ? TrimmedNonEmpty(qso.StationSnapshot.StationCallsign) : null)
            ?? TrimmedNonEmpty(snapshot.StationCallsign)
            ?? string.Empty;

        qso.StationCallsign = effectiveCallsign;
        if (!string.IsNullOrEmpty(effectiveCallsign))
        {
            snapshot.StationCallsign = effectiveCallsign;
        }

        NormalizeStationSnapshot(snapshot);
        qso.StationSnapshot = StationSnapshotHasValues(snapshot) ? snapshot : null;
    }

    public static void MaterializeStationSnapshotForUpdate(QsoRecord qso, QsoRecord? existing)
    {
        ArgumentNullException.ThrowIfNull(qso);

        var snapshot = existing is null
            ? new StationSnapshot()
            : EffectiveStationSnapshot(existing)?.Clone() ?? new StationSnapshot();

        if (qso.StationSnapshot is not null)
        {
            MergeStationSnapshot(snapshot, qso.StationSnapshot, clearBlankStrings: true);
        }

        var effectiveCallsign = TrimmedNonEmpty(qso.StationCallsign)
            ?? TrimmedNonEmpty(snapshot.StationCallsign)
            ?? string.Empty;

        qso.StationCallsign = effectiveCallsign;
        if (!string.IsNullOrEmpty(effectiveCallsign))
        {
            snapshot.StationCallsign = effectiveCallsign;
        }

        NormalizeStationSnapshot(snapshot);
        qso.StationSnapshot = StationSnapshotHasValues(snapshot) ? snapshot : null;
    }

    public static bool StationSnapshotHasValues(StationSnapshot snapshot)
    {
        ArgumentNullException.ThrowIfNull(snapshot);

        return TrimmedNonEmpty(snapshot.StationCallsign) is not null
            || TrimmedNonEmpty(snapshot.ProfileName) is not null
            || TrimmedNonEmpty(snapshot.OperatorCallsign) is not null
            || TrimmedNonEmpty(snapshot.OperatorName) is not null
            || TrimmedNonEmpty(snapshot.Grid) is not null
            || TrimmedNonEmpty(snapshot.County) is not null
            || TrimmedNonEmpty(snapshot.State) is not null
            || TrimmedNonEmpty(snapshot.Country) is not null
            || snapshot.Dxcc > 0
            || snapshot.CqZone > 0
            || snapshot.ItuZone > 0
            || snapshot.Latitude != 0
            || snapshot.Longitude != 0
            || TrimmedNonEmpty(snapshot.ArrlSection) is not null;
    }

    public static void NormalizeQsoForPersistence(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        qso.StationCallsign = qso.StationCallsign.Trim();
        qso.WorkedCallsign = qso.WorkedCallsign.Trim();
    }

    public static void ValidateQsoForPersistence(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        if (string.IsNullOrWhiteSpace(qso.StationCallsign))
        {
            throw new InvalidOperationException("station_callsign is required.");
        }

        if (string.IsNullOrWhiteSpace(qso.WorkedCallsign))
        {
            throw new InvalidOperationException("worked_callsign is required.");
        }

        if (qso.UtcTimestamp is null)
        {
            throw new InvalidOperationException("utc_timestamp is required.");
        }

        if (!System.Enum.IsDefined(qso.Band))
        {
            throw new InvalidOperationException("band is invalid.");
        }

        if (qso.Band == Band.Unspecified)
        {
            throw new InvalidOperationException("band is required.");
        }

        if (!System.Enum.IsDefined(qso.Mode))
        {
            throw new InvalidOperationException("mode is invalid.");
        }

        if (qso.Mode == Mode.Unspecified)
        {
            throw new InvalidOperationException("mode is required.");
        }
    }

    public static string? InvalidImportReason(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        if (string.IsNullOrWhiteSpace(qso.StationCallsign))
        {
            return "station_callsign is required.";
        }

        if (string.IsNullOrWhiteSpace(qso.WorkedCallsign))
        {
            return "worked_callsign is required.";
        }

        if (qso.UtcTimestamp is null)
        {
            if (qso.ExtraFields.TryGetValue("QSO_DATE", out var rawDate))
            {
                var suffix = qso.ExtraFields.TryGetValue("TIME_ON", out var rawTime)
                    ? $"/{rawTime}"
                    : string.Empty;
                return $"invalid ADIF date/time '{rawDate}{suffix}'.";
            }

            return "utc_timestamp is required.";
        }

        if (!System.Enum.IsDefined(qso.Band))
        {
            return "band is invalid.";
        }

        if (qso.Band == Band.Unspecified)
        {
            return qso.ExtraFields.TryGetValue("BAND", out var rawBand)
                ? $"unrecognized ADIF band '{rawBand}'."
                : "band is required.";
        }

        if (!System.Enum.IsDefined(qso.Mode))
        {
            return "mode is invalid.";
        }

        if (qso.Mode == Mode.Unspecified)
        {
            return qso.ExtraFields.TryGetValue("MODE", out var rawMode)
                ? $"unrecognized ADIF mode '{rawMode}'."
                : "mode is required.";
        }

        return null;
    }

    public static bool QsoHasStationContext(QsoRecord qso)
    {
        ArgumentNullException.ThrowIfNull(qso);

        return !string.IsNullOrWhiteSpace(qso.StationCallsign)
            || (qso.StationSnapshot is not null && StationSnapshotHasValues(qso.StationSnapshot));
    }

    public static string StationProfileLabel(StationProfile profile)
    {
        ArgumentNullException.ThrowIfNull(profile);

        return TrimmedNonEmpty(profile.ProfileName)
            ?? TrimmedNonEmpty(profile.StationCallsign)
            ?? "active station profile";
    }

    public static bool QsosMatchForDuplicate(QsoRecord existing, QsoRecord candidate)
    {
        ArgumentNullException.ThrowIfNull(existing);
        ArgumentNullException.ThrowIfNull(candidate);

        return TimestampsMatch(existing.UtcTimestamp, candidate.UtcTimestamp)
            && existing.Band == candidate.Band
            && existing.Mode == candidate.Mode
            && StringsEqualIgnoreAsciiCase(existing.StationCallsign, candidate.StationCallsign)
            && StringsEqualIgnoreAsciiCase(existing.WorkedCallsign, candidate.WorkedCallsign)
            && OptionalStringsCompatible(existing.HasSubmode ? existing.Submode : null, candidate.HasSubmode ? candidate.Submode : null)
            && OptionalUInt64Compatible(existing.HasFrequencyKhz ? existing.FrequencyKhz : null, candidate.HasFrequencyKhz ? candidate.FrequencyKhz : null);
    }

    /// <summary>
    /// Merges a partial update into an existing QSO record. Fields that are
    /// set to their protobuf default in <paramref name="incoming"/> are kept
    /// from <paramref name="existing"/>; non-default incoming values win.
    /// Identity fields (LocalId, QrzLogid, QrzBookid) and metadata timestamps
    /// are always preserved from the existing record.
    /// </summary>
    public static QsoRecord MergeQsoForUpdate(QsoRecord existing, QsoRecord incoming)
    {
        ArgumentNullException.ThrowIfNull(existing);
        ArgumentNullException.ThrowIfNull(incoming);

        var merged = existing.Clone();

        // --- Core contact fields (always required — take from incoming if non-default) ---
        if (!string.IsNullOrEmpty(incoming.StationCallsign))
        {
            merged.StationCallsign = incoming.StationCallsign;
        }

        if (!string.IsNullOrEmpty(incoming.WorkedCallsign))
        {
            merged.WorkedCallsign = incoming.WorkedCallsign;
        }

        if (incoming.UtcTimestamp is not null)
        {
            merged.UtcTimestamp = incoming.UtcTimestamp.Clone();
        }

        if (incoming.Band != Band.Unspecified)
        {
            merged.Band = incoming.Band;
        }

        if (incoming.Mode != Mode.Unspecified)
        {
            merged.Mode = incoming.Mode;
        }

        MergeOptionalUInt64(incoming.HasFrequencyKhz, incoming.FrequencyKhz, value => merged.FrequencyKhz = value);
        MergeOptionalString(incoming.HasSubmode ? incoming.Submode : null, value => merged.Submode = value);
        if (incoming.UtcEndTimestamp is not null)
        {
            merged.UtcEndTimestamp = incoming.UtcEndTimestamp.Clone();
        }

        // Station snapshot is handled separately by ApplyStationContextNoLock

        // --- Signal reports ---
        if (incoming.RstSent is not null)
        {
            merged.RstSent = incoming.RstSent.Clone();
        }

        if (incoming.RstReceived is not null)
        {
            merged.RstReceived = incoming.RstReceived.Clone();
        }

        MergeOptionalString(incoming.TxPower, value => merged.TxPower = value);

        // --- QSL / confirmation ---
        if (incoming.QslSentStatus != QslStatus.Unspecified)
        {
            merged.QslSentStatus = incoming.QslSentStatus;
        }

        if (incoming.QslReceivedStatus != QslStatus.Unspecified)
        {
            merged.QslReceivedStatus = incoming.QslReceivedStatus;
        }

        if (incoming.HasLotwSent)
        {
            merged.LotwSent = incoming.LotwSent;
        }

        if (incoming.HasLotwReceived)
        {
            merged.LotwReceived = incoming.LotwReceived;
        }

        if (incoming.HasEqslSent)
        {
            merged.EqslSent = incoming.EqslSent;
        }

        if (incoming.HasEqslReceived)
        {
            merged.EqslReceived = incoming.EqslReceived;
        }

        if (incoming.QslSentDate is not null)
        {
            merged.QslSentDate = incoming.QslSentDate.Clone();
        }

        if (incoming.QslReceivedDate is not null)
        {
            merged.QslReceivedDate = incoming.QslReceivedDate.Clone();
        }

        // --- Enrichment from callsign lookup ---
        MergeOptionalString(incoming.WorkedOperatorCallsign, value => merged.WorkedOperatorCallsign = value);
        MergeOptionalString(incoming.WorkedOperatorName, value => merged.WorkedOperatorName = value);
        MergeOptionalString(incoming.WorkedGrid, value => merged.WorkedGrid = value);
        MergeOptionalString(incoming.WorkedCountry, value => merged.WorkedCountry = value);
        MergeOptionalUInt32(incoming.HasWorkedDxcc, incoming.WorkedDxcc, value => merged.WorkedDxcc = value);
        MergeOptionalString(incoming.WorkedState, value => merged.WorkedState = value);
        MergeOptionalUInt32(incoming.HasWorkedCqZone, incoming.WorkedCqZone, value => merged.WorkedCqZone = value);
        MergeOptionalUInt32(incoming.HasWorkedItuZone, incoming.WorkedItuZone, value => merged.WorkedItuZone = value);
        MergeOptionalString(incoming.WorkedCounty, value => merged.WorkedCounty = value);
        MergeOptionalString(incoming.WorkedIota, value => merged.WorkedIota = value);
        MergeOptionalString(incoming.WorkedContinent, value => merged.WorkedContinent = value);
        MergeOptionalString(incoming.WorkedArrlSection, value => merged.WorkedArrlSection = value);
        MergeOptionalString(incoming.Skcc, value => merged.Skcc = value);

        // --- Contest fields ---
        MergeOptionalString(incoming.ContestId, value => merged.ContestId = value);
        MergeOptionalString(incoming.SerialSent, value => merged.SerialSent = value);
        MergeOptionalString(incoming.SerialReceived, value => merged.SerialReceived = value);
        MergeOptionalString(incoming.ExchangeSent, value => merged.ExchangeSent = value);
        MergeOptionalString(incoming.ExchangeReceived, value => merged.ExchangeReceived = value);

        // --- Propagation ---
        MergeOptionalString(incoming.PropMode, value => merged.PropMode = value);
        MergeOptionalString(incoming.SatName, value => merged.SatName = value);
        MergeOptionalString(incoming.SatMode, value => merged.SatMode = value);

        // --- Operator notes ---
        MergeOptionalString(incoming.Notes, value => merged.Notes = value);
        MergeOptionalString(incoming.Comment, value => merged.Comment = value);

        // --- Extra fields ---
        foreach (var pair in incoming.ExtraFields)
        {
            merged.ExtraFields[pair.Key] = pair.Value;
        }

        return merged;
    }

    public static QsoRecord MergeQsoForRefresh(QsoRecord existing, QsoRecord import)
    {
        ArgumentNullException.ThrowIfNull(existing);
        ArgumentNullException.ThrowIfNull(import);

        var merged = existing.Clone();
        merged.StationCallsign = import.StationCallsign;
        merged.WorkedCallsign = import.WorkedCallsign;
        merged.UtcTimestamp = import.UtcTimestamp?.Clone();
        merged.Band = import.Band;
        merged.Mode = import.Mode;

        MergeOptionalUInt64(import.HasFrequencyKhz, import.FrequencyKhz, value => merged.FrequencyKhz = value);
        MergeOptionalString(import.HasSubmode ? import.Submode : null, value => merged.Submode = value);
        if (import.UtcEndTimestamp is not null)
        {
            merged.UtcEndTimestamp = import.UtcEndTimestamp.Clone();
        }

        if (import.StationSnapshot is not null)
        {
            merged.StationSnapshot = import.StationSnapshot.Clone();
        }

        if (import.RstSent is not null)
        {
            merged.RstSent = import.RstSent.Clone();
        }

        if (import.RstReceived is not null)
        {
            merged.RstReceived = import.RstReceived.Clone();
        }

        MergeOptionalString(import.TxPower, value => merged.TxPower = value);
        MergeOptionalString(import.WorkedOperatorCallsign, value => merged.WorkedOperatorCallsign = value);
        MergeOptionalString(import.WorkedOperatorName, value => merged.WorkedOperatorName = value);
        MergeOptionalString(import.WorkedGrid, value => merged.WorkedGrid = value);
        MergeOptionalString(import.WorkedCountry, value => merged.WorkedCountry = value);
        MergeOptionalUInt32(import.HasWorkedDxcc, import.WorkedDxcc, value => merged.WorkedDxcc = value);
        MergeOptionalString(import.WorkedState, value => merged.WorkedState = value);
        MergeOptionalUInt32(import.HasWorkedCqZone, import.WorkedCqZone, value => merged.WorkedCqZone = value);
        MergeOptionalUInt32(import.HasWorkedItuZone, import.WorkedItuZone, value => merged.WorkedItuZone = value);
        MergeOptionalString(import.WorkedCounty, value => merged.WorkedCounty = value);
        MergeOptionalString(import.WorkedIota, value => merged.WorkedIota = value);
        MergeOptionalString(import.WorkedContinent, value => merged.WorkedContinent = value);
        MergeOptionalString(import.WorkedArrlSection, value => merged.WorkedArrlSection = value);

        MergeOptionalString(import.ContestId, value => merged.ContestId = value);
        MergeOptionalString(import.SerialSent, value => merged.SerialSent = value);
        MergeOptionalString(import.SerialReceived, value => merged.SerialReceived = value);
        MergeOptionalString(import.ExchangeSent, value => merged.ExchangeSent = value);
        MergeOptionalString(import.ExchangeReceived, value => merged.ExchangeReceived = value);

        MergeOptionalString(import.PropMode, value => merged.PropMode = value);
        MergeOptionalString(import.SatName, value => merged.SatName = value);
        MergeOptionalString(import.SatMode, value => merged.SatMode = value);

        MergeOptionalString(import.Notes, value => merged.Notes = value);
        MergeOptionalString(import.Comment, value => merged.Comment = value);

        foreach (var pair in import.ExtraFields)
        {
            merged.ExtraFields[pair.Key] = pair.Value;
        }

        merged.UpdatedAt = Timestamp.FromDateTimeOffset(DateTimeOffset.UtcNow);
        return merged;
    }

    private static void MergeStationSnapshot(StationSnapshot target, StationSnapshot overlay, bool clearBlankStrings)
    {
        MergeSnapshotOptionalString(value => target.ProfileName = value, overlay.ProfileName, clearBlankStrings);
        if (TrimmedNonEmpty(overlay.StationCallsign) is { } stationCallsign)
        {
            target.StationCallsign = stationCallsign;
        }

        MergeSnapshotOptionalString(value => target.OperatorCallsign = value, overlay.OperatorCallsign, clearBlankStrings);
        MergeSnapshotOptionalString(value => target.OperatorName = value, overlay.OperatorName, clearBlankStrings);
        MergeSnapshotOptionalString(value => target.Grid = value, overlay.Grid, clearBlankStrings);
        MergeSnapshotOptionalString(value => target.County = value, overlay.County, clearBlankStrings);
        MergeSnapshotOptionalString(value => target.State = value, overlay.State, clearBlankStrings);
        MergeSnapshotOptionalString(value => target.Country = value, overlay.Country, clearBlankStrings);
        MergeSnapshotOptionalString(value => target.ArrlSection = value, overlay.ArrlSection, clearBlankStrings);

        if (overlay.Dxcc > 0)
        {
            target.Dxcc = overlay.Dxcc;
        }

        if (overlay.CqZone > 0)
        {
            target.CqZone = overlay.CqZone;
        }

        if (overlay.ItuZone > 0)
        {
            target.ItuZone = overlay.ItuZone;
        }

        if (overlay.Latitude != 0)
        {
            target.Latitude = overlay.Latitude;
        }

        if (overlay.Longitude != 0)
        {
            target.Longitude = overlay.Longitude;
        }
    }

    private static void NormalizeStationSnapshot(StationSnapshot snapshot)
    {
        snapshot.ProfileName = NormalizeOptionalString(snapshot.ProfileName);
        snapshot.StationCallsign = NormalizeRequiredString(snapshot.StationCallsign);
        snapshot.OperatorCallsign = NormalizeOptionalString(snapshot.OperatorCallsign);
        snapshot.OperatorName = NormalizeOptionalString(snapshot.OperatorName);
        snapshot.Grid = NormalizeOptionalString(snapshot.Grid);
        snapshot.County = NormalizeOptionalString(snapshot.County);
        snapshot.State = NormalizeOptionalString(snapshot.State);
        snapshot.Country = NormalizeOptionalString(snapshot.Country);
        snapshot.ArrlSection = NormalizeOptionalString(snapshot.ArrlSection);

    }

    private static void MergeSnapshotOptionalString(Action<string?> setter, string? source, bool clearBlankStrings)
    {
        var normalized = TrimmedNonEmpty(source);
        if (normalized is not null)
        {
            setter(normalized);
            return;
        }

        if (clearBlankStrings && source is not null)
        {
            setter(string.Empty);
        }
    }

    private static void MergeOptionalString(string? source, Action<string> setter)
    {
        if (TrimmedNonEmpty(source) is { } value)
        {
            setter(value);
        }
    }

    private static void MergeOptionalUInt32(bool hasValue, uint source, Action<uint> setter)
    {
        if (hasValue && source > 0)
        {
            setter(source);
        }
    }

    private static void MergeOptionalUInt64(bool hasValue, ulong source, Action<ulong> setter)
    {
        if (hasValue && source > 0)
        {
            setter(source);
        }
    }

    private static bool TimestampsMatch(Timestamp? left, Timestamp? right)
    {
        return Equals(left, right);
    }

    private static bool StringsEqualIgnoreAsciiCase(string left, string right)
    {
        return left.Trim().Equals(right.Trim(), StringComparison.OrdinalIgnoreCase);
    }

    private static bool OptionalStringsCompatible(string? left, string? right)
    {
        var normalizedLeft = TrimmedNonEmpty(left);
        var normalizedRight = TrimmedNonEmpty(right);
        return normalizedLeft is null
            || normalizedRight is null
            || normalizedLeft.Equals(normalizedRight, StringComparison.OrdinalIgnoreCase);
    }

    private static bool OptionalUInt64Compatible(ulong? left, ulong? right)
    {
        return left is null || right is null || left.Value == right.Value;
    }

    private static string NormalizeOptionalString(string? value)
    {
        return TrimmedNonEmpty(value) ?? string.Empty;
    }

    private static string NormalizeRequiredString(string value)
    {
        return value.Trim();
    }

    private static string? TrimmedNonEmpty(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        return value.Trim();
    }
}
