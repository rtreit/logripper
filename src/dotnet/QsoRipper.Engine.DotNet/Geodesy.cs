using System;
using QsoRipper.Domain;

namespace QsoRipper.Engine.DotNet;

/// <summary>
/// Spherical-Earth geodesy mirror of the Rust <c>qsoripper-core::geodesy</c>
/// module. Used by <see cref="ManagedGreatCircleGrpcService"/> to compute
/// distances, bearings, and great-circle samples for the
/// <c>GreatCircleService</c> RPC. Cross-engine results match within
/// floating-point tolerance.
/// </summary>
internal static class Geodesy
{
    public const double EarthRadiusKm = 6371.0088;
    public const uint DefaultSampleCount = 64;
    public const uint MinSampleCount = 2;
    public const uint MaxSampleCount = 512;

    /// <summary>
    /// Validate latitude/longitude. Throws <see cref="ArgumentException"/>
    /// for NaN, infinite, or out-of-range values.
    /// </summary>
    public static void ValidatePoint(GeoPoint point, string label)
    {
        ArgumentNullException.ThrowIfNull(point);
        if (double.IsNaN(point.Latitude) || double.IsInfinity(point.Latitude)
            || point.Latitude < -90.0 || point.Latitude > 90.0)
        {
            throw new ArgumentException(
                $"{label}: latitude {point.Latitude} is out of range or non-finite",
                nameof(point));
        }

        if (double.IsNaN(point.Longitude) || double.IsInfinity(point.Longitude)
            || point.Longitude < -180.0 || point.Longitude > 180.0)
        {
            throw new ArgumentException(
                $"{label}: longitude {point.Longitude} is out of range or non-finite",
                nameof(point));
        }
    }

    /// <summary>
    /// Resolve a Maidenhead locator (4, 6, or 8 chars; case-insensitive) to
    /// the center point of the locator's cell.
    /// </summary>
    public static GeoPoint MaidenheadToGeoPoint(string locator)
    {
        ArgumentNullException.ThrowIfNull(locator);
        var trimmed = locator.Trim().ToUpperInvariant();
        if (trimmed.Length is not (4 or 6 or 8))
        {
            throw new ArgumentException(
                $"invalid Maidenhead locator: expected 4, 6, or 8 characters, got {trimmed.Length}",
                nameof(locator));
        }

        int FieldDigit(char c, char lo, char hi, string desc)
        {
            if (c < lo || c > hi)
            {
                throw new ArgumentException($"invalid Maidenhead locator: bad {desc} '{c}'", nameof(locator));
            }
            return c - lo;
        }

        int Decimal(char c, string desc)
        {
            if (c < '0' || c > '9')
            {
                throw new ArgumentException($"invalid Maidenhead locator: bad {desc} '{c}'", nameof(locator));
            }
            return c - '0';
        }

        var fieldLon = FieldDigit(trimmed[0], 'A', 'R', "field");
        var fieldLat = FieldDigit(trimmed[1], 'A', 'R', "field");
        var lon = -180.0 + fieldLon * 20.0;
        var lat = -90.0 + fieldLat * 10.0;

        var sqLon = Decimal(trimmed[2], "square digit");
        var sqLat = Decimal(trimmed[3], "square digit");
        lon += sqLon * 2.0;
        lat += sqLat * 1.0;

        var lonStep = 2.0;
        var latStep = 1.0;

        if (trimmed.Length >= 6)
        {
            var subLon = FieldDigit(trimmed[4], 'A', 'X', "subsquare");
            var subLat = FieldDigit(trimmed[5], 'A', 'X', "subsquare");
            lonStep = 2.0 / 24.0;
            latStep = 1.0 / 24.0;
            lon += subLon * lonStep;
            lat += subLat * latStep;
        }

        if (trimmed.Length == 8)
        {
            var extLon = Decimal(trimmed[6], "extended digit");
            var extLat = Decimal(trimmed[7], "extended digit");
            lonStep /= 10.0;
            latStep /= 10.0;
            lon += extLon * lonStep;
            lat += extLat * latStep;
        }

        return new GeoPoint
        {
            Latitude = lat + (latStep / 2.0),
            Longitude = lon + (lonStep / 2.0),
        };
    }

    public static double DistanceKm(GeoPoint origin, GeoPoint target)
    {
        ArgumentNullException.ThrowIfNull(origin);
        ArgumentNullException.ThrowIfNull(target);
        var lat1 = ToRad(origin.Latitude);
        var lat2 = ToRad(target.Latitude);
        var dLat = ToRad(target.Latitude - origin.Latitude);
        var dLon = ToRad(target.Longitude - origin.Longitude);
        var a = (Math.Sin(dLat / 2) * Math.Sin(dLat / 2))
            + (Math.Cos(lat1) * Math.Cos(lat2) * Math.Sin(dLon / 2) * Math.Sin(dLon / 2));
        return 2.0 * EarthRadiusKm * Math.Atan2(Math.Sqrt(a), Math.Sqrt(1.0 - a));
    }

    /// <summary>
    /// Initial bearing in degrees (clockwise from true north). Returns
    /// <c>null</c> for coincident or antipodal endpoints where the bearing
    /// is undefined.
    /// </summary>
    public static double? InitialBearingDeg(GeoPoint origin, GeoPoint target)
    {
        ArgumentNullException.ThrowIfNull(origin);
        ArgumentNullException.ThrowIfNull(target);
        if (BearingUndefined(origin, target))
        {
            return null;
        }

        var lat1 = ToRad(origin.Latitude);
        var lat2 = ToRad(target.Latitude);
        var dLon = ToRad(target.Longitude - origin.Longitude);
        var y = Math.Sin(dLon) * Math.Cos(lat2);
        var x = (Math.Cos(lat1) * Math.Sin(lat2)) - (Math.Sin(lat1) * Math.Cos(lat2) * Math.Cos(dLon));
        return NormalizeBearing(ToDeg(Math.Atan2(y, x)));
    }

    public static double? FinalBearingDeg(GeoPoint origin, GeoPoint target)
    {
        var reverse = InitialBearingDeg(target, origin);
        return reverse.HasValue ? NormalizeBearing(reverse.Value + 180.0) : null;
    }

    /// <summary>
    /// Spherical linear interpolation along the great circle.
    /// Returns <paramref name="count"/> evenly spaced points (including endpoints).
    /// Caller must pass <paramref name="count"/> &gt;= 2.
    /// </summary>
    public static GeoPoint[] SampleGreatCircle(GeoPoint origin, GeoPoint target, uint count)
    {
        ArgumentNullException.ThrowIfNull(origin);
        ArgumentNullException.ThrowIfNull(target);
        var n = Math.Max(2u, count);
        var lat1 = ToRad(origin.Latitude);
        var lon1 = ToRad(origin.Longitude);
        var lat2 = ToRad(target.Latitude);
        var lon2 = ToRad(target.Longitude);
        var p1 = ToXyz(lat1, lon1);
        var p2 = ToXyz(lat2, lon2);

        var dot = Math.Clamp((p1.X * p2.X) + (p1.Y * p2.Y) + (p1.Z * p2.Z), -1.0, 1.0);
        var omega = Math.Acos(dot);
        var sinOmega = Math.Sin(omega);

        var samples = new GeoPoint[n];

        if (Math.Abs(sinOmega) < 1e-9)
        {
            for (var i = 0u; i < n; i++)
            {
                var t = (double)i / (n - 1);
                samples[i] = t < 0.5
                    ? new GeoPoint { Latitude = origin.Latitude, Longitude = origin.Longitude }
                    : new GeoPoint { Latitude = target.Latitude, Longitude = target.Longitude };
            }

            return samples;
        }

        for (var i = 0u; i < n; i++)
        {
            var t = (double)i / (n - 1);
            var a = Math.Sin((1.0 - t) * omega) / sinOmega;
            var b = Math.Sin(t * omega) / sinOmega;
            var x = (a * p1.X) + (b * p2.X);
            var y = (a * p1.Y) + (b * p2.Y);
            var z = (a * p1.Z) + (b * p2.Z);
            var lat = Math.Asin(z);
            var lon = Math.Atan2(y, x);
            samples[i] = new GeoPoint
            {
                Latitude = ToDeg(lat),
                Longitude = ToDeg(lon),
            };
        }

        return samples;
    }

    /// <summary>Resolve and validate the requested sample count.</summary>
    public static uint ResolveSampleCount(uint requested)
    {
        if (requested == 0)
        {
            return DefaultSampleCount;
        }
        if (requested < MinSampleCount || requested > MaxSampleCount)
        {
            throw new ArgumentOutOfRangeException(
                nameof(requested),
                requested,
                $"sample_count must be 0 (default) or in [{MinSampleCount}, {MaxSampleCount}]");
        }
        return requested;
    }

    private static bool BearingUndefined(GeoPoint origin, GeoPoint target)
    {
        var same = Math.Abs(origin.Latitude - target.Latitude) < 1e-9
            && Math.Abs(origin.Longitude - target.Longitude) < 1e-9;
        var antipodal = Math.Abs(origin.Latitude + target.Latitude) < 1e-9
            && Math.Abs(Math.Abs(origin.Longitude - target.Longitude) - 180.0) < 1e-9;
        return same || antipodal;
    }

    private static double NormalizeBearing(double deg)
    {
        var normalized = deg - (Math.Floor(deg / 360.0) * 360.0);
        return double.IsNaN(normalized) ? 0.0 : normalized;
    }

    private static (double X, double Y, double Z) ToXyz(double lat, double lon)
    {
        var cosLat = Math.Cos(lat);
        return (cosLat * Math.Cos(lon), cosLat * Math.Sin(lon), Math.Sin(lat));
    }

    private static double ToRad(double deg) => deg * Math.PI / 180.0;

    private static double ToDeg(double rad) => rad * 180.0 / Math.PI;
}
