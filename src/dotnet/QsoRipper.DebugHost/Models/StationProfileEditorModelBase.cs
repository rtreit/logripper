using System.ComponentModel.DataAnnotations;
using QsoRipper.Domain;

namespace QsoRipper.DebugHost.Models;

internal abstract class StationProfileEditorModelBase : IValidatableObject
{
    public string? ProfileName { get; set; }

    public string StationCallsign { get; set; } = string.Empty;

    public string? OperatorCallsign { get; set; }

    public string? OperatorName { get; set; }

    public string? Grid { get; set; }

    public string? County { get; set; }

    public string? State { get; set; }

    public string? Country { get; set; }

    public int? Dxcc { get; set; }

    public int? CqZone { get; set; }

    public int? ItuZone { get; set; }

    public double? Latitude { get; set; }

    public double? Longitude { get; set; }

    public void LoadFrom(StationProfile? profile)
    {
        if (profile is null)
        {
            ResetStationFields();
            return;
        }

        ProfileName = NormalizeOptional(profile.ProfileName);
        StationCallsign = profile.StationCallsign?.Trim() ?? string.Empty;
        OperatorCallsign = NormalizeOptional(profile.OperatorCallsign);
        OperatorName = NormalizeOptional(profile.OperatorName);
        Grid = NormalizeOptional(profile.Grid);
        County = NormalizeOptional(profile.County);
        State = NormalizeOptional(profile.State);
        Country = NormalizeOptional(profile.Country);
        Dxcc = profile.HasDxcc ? checked((int)profile.Dxcc) : null;
        CqZone = profile.HasCqZone ? checked((int)profile.CqZone) : null;
        ItuZone = profile.HasItuZone ? checked((int)profile.ItuZone) : null;
        Latitude = profile.HasLatitude ? profile.Latitude : null;
        Longitude = profile.HasLongitude ? profile.Longitude : null;
    }

    public StationProfile ToStationProfile()
    {
        var profile = new StationProfile
        {
            StationCallsign = StationCallsign.Trim()
        };

        SetOptionalString(
            NormalizeOptional(ProfileName),
            value => profile.ProfileName = value,
            profile.ClearProfileName);
        SetOptionalString(
            NormalizeOptional(OperatorCallsign),
            value => profile.OperatorCallsign = value,
            profile.ClearOperatorCallsign);
        SetOptionalString(
            NormalizeOptional(OperatorName),
            value => profile.OperatorName = value,
            profile.ClearOperatorName);
        SetOptionalString(
            NormalizeOptional(Grid),
            value => profile.Grid = value,
            profile.ClearGrid);
        SetOptionalString(
            NormalizeOptional(County),
            value => profile.County = value,
            profile.ClearCounty);
        SetOptionalString(
            NormalizeOptional(State),
            value => profile.State = value,
            profile.ClearState);
        SetOptionalString(
            NormalizeOptional(Country),
            value => profile.Country = value,
            profile.ClearCountry);
        SetOptionalValue(Dxcc, value => profile.Dxcc = checked((uint)value), profile.ClearDxcc);
        SetOptionalValue(CqZone, value => profile.CqZone = checked((uint)value), profile.ClearCqZone);
        SetOptionalValue(ItuZone, value => profile.ItuZone = checked((uint)value), profile.ClearItuZone);
        SetOptionalValue(Latitude, value => profile.Latitude = value, profile.ClearLatitude);
        SetOptionalValue(Longitude, value => profile.Longitude = value, profile.ClearLongitude);

        return profile;
    }

    public virtual IEnumerable<ValidationResult> Validate(ValidationContext validationContext)
    {
        if (string.IsNullOrWhiteSpace(StationCallsign))
        {
            yield return new ValidationResult(
                "Station callsign is required.",
                [nameof(StationCallsign)]);
        }

        if (Latitude is < -90 or > 90)
        {
            yield return new ValidationResult(
                "Latitude must be between -90 and 90.",
                [nameof(Latitude)]);
        }

        if (Longitude is < -180 or > 180)
        {
            yield return new ValidationResult(
                "Longitude must be between -180 and 180.",
                [nameof(Longitude)]);
        }

        if (Dxcc is <= 0)
        {
            yield return new ValidationResult(
                "DXCC must be greater than zero when provided.",
                [nameof(Dxcc)]);
        }

        if (CqZone is <= 0)
        {
            yield return new ValidationResult(
                "CQ zone must be greater than zero when provided.",
                [nameof(CqZone)]);
        }

        if (ItuZone is <= 0)
        {
            yield return new ValidationResult(
                "ITU zone must be greater than zero when provided.",
                [nameof(ItuZone)]);
        }
    }

    protected void ResetStationFields()
    {
        ProfileName = null;
        StationCallsign = string.Empty;
        OperatorCallsign = null;
        OperatorName = null;
        Grid = null;
        County = null;
        State = null;
        Country = null;
        Dxcc = null;
        CqZone = null;
        ItuZone = null;
        Latitude = null;
        Longitude = null;
    }

    protected static string? NormalizeOptional(string? value)
    {
        return string.IsNullOrWhiteSpace(value)
            ? null
            : value.Trim();
    }

    private static void SetOptionalString(string? value, Action<string> setter, Action clear)
    {
        if (value is null)
        {
            clear();
            return;
        }

        setter(value);
    }

    private static void SetOptionalValue<T>(T? value, Action<T> setter, Action clear)
        where T : struct
    {
        if (value is null)
        {
            clear();
            return;
        }

        setter(value.Value);
    }
}
