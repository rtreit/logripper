//! Spherical-Earth geodesy: distance, bearing, and great-circle interpolation
//! used by the engine's `GreatCircleService`.
//!
//! All inputs and outputs are decimal degrees. Computations use the spherical
//! Earth model with mean radius `R = 6371.0088 km` (IUGG mean Earth radius).
//! This is sufficient for displaying contact distance/bearing on a UI map;
//! ellipsoidal precision (Vincenty/Karney) is not needed for those purposes.

use crate::proto::qsoripper::domain::GeoPoint;

/// Mean Earth radius in kilometres (IUGG/Wikipedia mean radius).
pub const EARTH_RADIUS_KM: f64 = 6371.0088;

/// Default number of geodesic samples returned when the caller asks for `0`.
pub const DEFAULT_SAMPLE_COUNT: u32 = 64;
/// Minimum supported sample count. A geodesic always includes both endpoints,
/// so the request must ask for at least 2 samples.
pub const MIN_SAMPLE_COUNT: u32 = 2;
/// Maximum supported sample count. Caps abuse without sacrificing visual
/// smoothness — 512 points is enough for a sub-pixel arc on a 4K display.
pub const MAX_SAMPLE_COUNT: u32 = 512;

/// Errors returned by the geodesy module. Mapped to gRPC `INVALID_ARGUMENT`
/// at the service boundary.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GeodesyError {
    /// Latitude was outside `[-90, 90]` or non-finite.
    #[error("latitude {0} is out of range or non-finite")]
    LatitudeOutOfRange(String),
    /// Longitude was outside `[-180, 180]` or non-finite.
    #[error("longitude {0} is out of range or non-finite")]
    LongitudeOutOfRange(String),
    /// Sample count was below `MIN_SAMPLE_COUNT` or above `MAX_SAMPLE_COUNT`.
    #[error("sample_count {0} must be 0 (default) or in [{MIN_SAMPLE_COUNT}, {MAX_SAMPLE_COUNT}]")]
    SampleCountOutOfRange(u32),
    /// Maidenhead locator was empty, the wrong length, or contained invalid
    /// characters.
    #[error("invalid Maidenhead locator: {0}")]
    InvalidMaidenhead(String),
}

/// Validate a `GeoPoint`'s latitude and longitude.
///
/// # Errors
/// Returns `GeodesyError::LatitudeOutOfRange` or `LongitudeOutOfRange`
/// when the coordinates are non-finite or fall outside `[-90, 90]` /
/// `[-180, 180]`.
pub fn validate_point(point: &GeoPoint) -> Result<(), GeodesyError> {
    if !point.latitude.is_finite() || !(-90.0..=90.0).contains(&point.latitude) {
        return Err(GeodesyError::LatitudeOutOfRange(point.latitude.to_string()));
    }
    if !point.longitude.is_finite() || !(-180.0..=180.0).contains(&point.longitude) {
        return Err(GeodesyError::LongitudeOutOfRange(
            point.longitude.to_string(),
        ));
    }
    Ok(())
}

/// Resolve a Maidenhead locator (4, 6, or 8 characters) to its center
/// coordinates in decimal degrees. Locators are case-insensitive.
///
/// The classic Maidenhead system encodes longitude/latitude pairs as:
///   - field   (A..R)  → 20° × 10°
///   - square  (0..9)  → 2°  × 1°
///   - subsquare (a..x) → 5'  × 2.5'
///   - extended-subsquare (0..9) → 30" × 15"
///
/// # Errors
/// Returns `GeodesyError::InvalidMaidenhead` when the locator is the wrong
/// length or contains a character outside the expected range for its
/// position.
#[allow(clippy::indexing_slicing, clippy::similar_names)]
pub fn maidenhead_to_geopoint(locator: &str) -> Result<GeoPoint, GeodesyError> {
    let trimmed = locator.trim();
    let upper: Vec<char> = trimmed.to_ascii_uppercase().chars().collect();
    let len = upper.len();
    if !matches!(len, 4 | 6 | 8) {
        return Err(GeodesyError::InvalidMaidenhead(format!(
            "expected 4, 6, or 8 characters, got {len}"
        )));
    }
    // Field pair: A..R
    let field_lon = char_in_range(upper[0], 'A', 'R')
        .ok_or_else(|| GeodesyError::InvalidMaidenhead(format!("bad field char {}", upper[0])))?;
    let field_lat = char_in_range(upper[1], 'A', 'R')
        .ok_or_else(|| GeodesyError::InvalidMaidenhead(format!("bad field char {}", upper[1])))?;
    let mut lon = -180.0_f64 + f64::from(field_lon) * 20.0;
    let mut lat = -90.0_f64 + f64::from(field_lat) * 10.0;
    // Square pair: 0..9
    let sq_lon = digit(upper[2])
        .ok_or_else(|| GeodesyError::InvalidMaidenhead(format!("bad square digit {}", upper[2])))?;
    let sq_lat = digit(upper[3])
        .ok_or_else(|| GeodesyError::InvalidMaidenhead(format!("bad square digit {}", upper[3])))?;
    lon += f64::from(sq_lon) * 2.0;
    lat += f64::from(sq_lat) * 1.0;
    // Half cell width at this resolution (4-char locator = 2° lon × 1° lat).
    let mut lon_step = 2.0_f64;
    let mut lat_step = 1.0_f64;

    if len >= 6 {
        // Subsquare: A..X (need lowercase form for spec but we already uppered).
        let sub_lon = char_in_range(upper[4], 'A', 'X').ok_or_else(|| {
            GeodesyError::InvalidMaidenhead(format!("bad subsquare char {}", upper[4]))
        })?;
        let sub_lat = char_in_range(upper[5], 'A', 'X').ok_or_else(|| {
            GeodesyError::InvalidMaidenhead(format!("bad subsquare char {}", upper[5]))
        })?;
        lon_step = 2.0 / 24.0;
        lat_step = 1.0 / 24.0;
        lon += f64::from(sub_lon) * lon_step;
        lat += f64::from(sub_lat) * lat_step;
    }

    if len == 8 {
        let ext_lon = digit(upper[6]).ok_or_else(|| {
            GeodesyError::InvalidMaidenhead(format!("bad extended digit {}", upper[6]))
        })?;
        let ext_lat = digit(upper[7]).ok_or_else(|| {
            GeodesyError::InvalidMaidenhead(format!("bad extended digit {}", upper[7]))
        })?;
        lon_step /= 10.0;
        lat_step /= 10.0;
        lon += f64::from(ext_lon) * lon_step;
        lat += f64::from(ext_lat) * lat_step;
    }

    // Center of the cell: half a step in each direction.
    Ok(GeoPoint {
        latitude: lat + lat_step / 2.0,
        longitude: lon + lon_step / 2.0,
    })
}

fn char_in_range(c: char, lo: char, hi: char) -> Option<u32> {
    if (lo..=hi).contains(&c) {
        Some(u32::from(c) - u32::from(lo))
    } else {
        None
    }
}

fn digit(c: char) -> Option<u32> {
    c.to_digit(10)
}

/// Great-circle (Haversine) distance in kilometres between two points.
#[must_use]
pub fn distance_km(origin: &GeoPoint, target: &GeoPoint) -> f64 {
    let lat1 = origin.latitude.to_radians();
    let lat2 = target.latitude.to_radians();
    let dlat = (target.latitude - origin.latitude).to_radians();
    let dlon = (target.longitude - origin.longitude).to_radians();
    let a = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().atan2((1.0 - a).sqrt())
}

/// Initial bearing in degrees from `origin` toward `target`, clockwise from
/// true north in `[0, 360)`. Returns `None` for coincident or antipodal
/// points where the bearing is undefined.
#[must_use]
pub fn initial_bearing_deg(origin: &GeoPoint, target: &GeoPoint) -> Option<f64> {
    if bearing_undefined(origin, target) {
        return None;
    }
    let lat1 = origin.latitude.to_radians();
    let lat2 = target.latitude.to_radians();
    let dlon = (target.longitude - origin.longitude).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    Some(normalize_bearing(y.atan2(x).to_degrees()))
}

/// Final bearing in degrees at `target` of a great circle from `origin`.
/// Equivalent to the initial bearing of the reverse path, plus 180°.
#[must_use]
pub fn final_bearing_deg(origin: &GeoPoint, target: &GeoPoint) -> Option<f64> {
    initial_bearing_deg(target, origin).map(|b| normalize_bearing(b + 180.0))
}

fn bearing_undefined(origin: &GeoPoint, target: &GeoPoint) -> bool {
    // Same point => no direction.
    let same = (origin.latitude - target.latitude).abs() < 1e-9
        && (origin.longitude - target.longitude).abs() < 1e-9;
    // Antipodal: lat sum ≈ 0 AND |lon diff| ≈ 180.
    let antipodal = (origin.latitude + target.latitude).abs() < 1e-9
        && ((origin.longitude - target.longitude).abs() - 180.0).abs() < 1e-9;
    same || antipodal
}

fn normalize_bearing(deg: f64) -> f64 {
    let normalized = deg.rem_euclid(360.0);
    if normalized.is_nan() {
        0.0
    } else {
        normalized
    }
}

/// Spherical linear interpolation along the great circle from `origin`
/// to `target`. Returns `count` evenly spaced points (including endpoints).
/// `count` must be at least 2.
#[must_use]
#[allow(clippy::many_single_char_names, clippy::similar_names)]
pub fn sample_great_circle(origin: &GeoPoint, target: &GeoPoint, count: u32) -> Vec<GeoPoint> {
    let count = count.max(2);
    // Convert endpoints to unit-vector cartesian on the unit sphere.
    let (lat1, lon1) = (origin.latitude.to_radians(), origin.longitude.to_radians());
    let (lat2, lon2) = (target.latitude.to_radians(), target.longitude.to_radians());
    let p1 = lat_lon_to_xyz(lat1, lon1);
    let p2 = lat_lon_to_xyz(lat2, lon2);

    // Angular distance between the two points.
    let dot = (p1.0 * p2.0 + p1.1 * p2.1 + p1.2 * p2.2).clamp(-1.0, 1.0);
    let omega = dot.acos();
    let sin_omega = omega.sin();

    let mut out = Vec::with_capacity(count as usize);
    let last = f64::from(count - 1);

    // Degenerate cases: identical points or antipodal points => can't slerp.
    // Fall back to the endpoints repeated/linearly mixed; antipodal great
    // circles are non-unique so the rendered arc isn't meaningful anyway.
    if sin_omega.abs() < 1e-9 {
        for i in 0..count {
            let t = f64::from(i) / last;
            out.push(if t < 0.5 { *origin } else { *target });
        }
        return out;
    }

    for i in 0..count {
        let t = f64::from(i) / last;
        let a = ((1.0 - t) * omega).sin() / sin_omega;
        let b = (t * omega).sin() / sin_omega;
        let x = a * p1.0 + b * p2.0;
        let y = a * p1.1 + b * p2.1;
        let z = a * p1.2 + b * p2.2;
        let lat = z.asin();
        let lon = y.atan2(x);
        out.push(GeoPoint {
            latitude: lat.to_degrees(),
            longitude: lon.to_degrees(),
        });
    }
    out
}

fn lat_lon_to_xyz(lat: f64, lon: f64) -> (f64, f64, f64) {
    let cos_lat = lat.cos();
    (cos_lat * lon.cos(), cos_lat * lon.sin(), lat.sin())
}

/// Resolve and clamp the caller-requested sample count.
///
///   - 0          => `DEFAULT_SAMPLE_COUNT`
///   - 1          => error
///   - 2..=MAX    => honored
///   - > MAX      => error
///
/// # Errors
/// Returns `GeodesyError::SampleCountOutOfRange` when the request is `1`
/// or above `MAX_SAMPLE_COUNT`.
pub fn resolve_sample_count(requested: u32) -> Result<u32, GeodesyError> {
    if requested == 0 {
        Ok(DEFAULT_SAMPLE_COUNT)
    } else if (MIN_SAMPLE_COUNT..=MAX_SAMPLE_COUNT).contains(&requested) {
        Ok(requested)
    } else {
        Err(GeodesyError::SampleCountOutOfRange(requested))
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::similar_names,
    clippy::many_single_char_names
)]
mod tests {
    use super::*;

    fn pt(lat: f64, lon: f64) -> GeoPoint {
        GeoPoint {
            latitude: lat,
            longitude: lon,
        }
    }

    #[test]
    fn validate_accepts_normal_points() {
        assert!(validate_point(&pt(0.0, 0.0)).is_ok());
        assert!(validate_point(&pt(89.999, 179.999)).is_ok());
        assert!(validate_point(&pt(-90.0, -180.0)).is_ok());
    }

    #[test]
    fn validate_rejects_out_of_range_or_nan() {
        assert!(matches!(
            validate_point(&pt(91.0, 0.0)),
            Err(GeodesyError::LatitudeOutOfRange(_))
        ));
        assert!(matches!(
            validate_point(&pt(0.0, 181.0)),
            Err(GeodesyError::LongitudeOutOfRange(_))
        ));
        assert!(matches!(
            validate_point(&pt(f64::NAN, 0.0)),
            Err(GeodesyError::LatitudeOutOfRange(_))
        ));
        assert!(matches!(
            validate_point(&pt(0.0, f64::INFINITY)),
            Err(GeodesyError::LongitudeOutOfRange(_))
        ));
    }

    #[test]
    fn distance_seattle_to_london_is_about_7700_km() {
        // Seattle (KSEA) ≈ 47.45 N, -122.31; London (Heathrow) ≈ 51.47 N, -0.46.
        let d = distance_km(&pt(47.45, -122.31), &pt(51.47, -0.46));
        assert!((d - 7720.0).abs() < 30.0, "got {d}");
    }

    #[test]
    fn distance_paris_to_new_york_is_about_5837_km() {
        let d = distance_km(&pt(48.8566, 2.3522), &pt(40.7128, -74.0060));
        assert!((d - 5837.0).abs() < 20.0, "got {d}");
    }

    #[test]
    fn distance_to_self_is_zero() {
        let d = distance_km(&pt(34.0, -118.0), &pt(34.0, -118.0));
        assert!(d < 1e-6, "got {d}");
    }

    #[test]
    fn initial_bearing_known_pairs() {
        // Seattle → New York: roughly east-southeast (~78°).
        let b = initial_bearing_deg(&pt(47.45, -122.31), &pt(40.7128, -74.0060)).unwrap();
        assert!((b - 78.0).abs() < 5.0, "got {b}");

        // Paris → New York: roughly west-northwest (~291°).
        let b = initial_bearing_deg(&pt(48.8566, 2.3522), &pt(40.7128, -74.0060)).unwrap();
        assert!((b - 291.0).abs() < 5.0, "got {b}");
    }

    #[test]
    fn bearing_undefined_for_same_or_antipodal_points() {
        assert!(initial_bearing_deg(&pt(0.0, 0.0), &pt(0.0, 0.0)).is_none());
        assert!(initial_bearing_deg(&pt(10.0, 20.0), &pt(-10.0, -160.0)).is_none());
    }

    #[test]
    fn samples_endpoints_match() {
        let o = pt(47.45, -122.31);
        let t = pt(40.7128, -74.0060);
        let samples = sample_great_circle(&o, &t, 16);
        assert_eq!(samples.len(), 16);
        assert!((samples[0].latitude - o.latitude).abs() < 1e-6);
        assert!((samples[0].longitude - o.longitude).abs() < 1e-6);
        assert!((samples[15].latitude - t.latitude).abs() < 1e-6);
        assert!((samples[15].longitude - t.longitude).abs() < 1e-6);
    }

    #[test]
    fn samples_distance_matches_total() {
        let o = pt(47.45, -122.31);
        let t = pt(40.7128, -74.0060);
        let samples = sample_great_circle(&o, &t, 64);
        let mut sum = 0.0;
        for w in samples.windows(2) {
            sum += distance_km(&w[0], &w[1]);
        }
        let total = distance_km(&o, &t);
        // Polyline sum should match the geodesic distance to high precision.
        assert!(
            (sum - total).abs() / total < 1e-3,
            "sum={sum} total={total}"
        );
    }

    #[test]
    fn maidenhead_4char_center() {
        // CN87 = field C(2), N(13), square 8, 7. Cell spans lon -124..-122
        // and lat 47..48; the center is therefore (-123, 47.5).
        let p = maidenhead_to_geopoint("CN87").unwrap();
        assert!((p.longitude - (-123.0)).abs() < 1e-6, "lon={}", p.longitude);
        assert!((p.latitude - 47.5).abs() < 1e-6, "lat={}", p.latitude);
    }

    #[test]
    fn maidenhead_6char_center_in_4char_cell() {
        let p4 = maidenhead_to_geopoint("CN87").unwrap();
        let p6 = maidenhead_to_geopoint("CN87wn").unwrap();
        assert!((p6.longitude - p4.longitude).abs() < 2.0);
        assert!((p6.latitude - p4.latitude).abs() < 1.0);
    }

    #[test]
    fn maidenhead_case_insensitive() {
        let upper = maidenhead_to_geopoint("CN87WN").unwrap();
        let lower = maidenhead_to_geopoint("cn87wn").unwrap();
        assert!((upper.latitude - lower.latitude).abs() < 1e-9);
        assert!((upper.longitude - lower.longitude).abs() < 1e-9);
    }

    #[test]
    fn maidenhead_8char_center_in_6char_cell() {
        let p6 = maidenhead_to_geopoint("CN87wn").unwrap();
        let p8 = maidenhead_to_geopoint("CN87wn46").unwrap();
        assert!((p8.longitude - p6.longitude).abs() < (2.0 / 24.0));
        assert!((p8.latitude - p6.latitude).abs() < (1.0 / 24.0));
    }

    #[test]
    fn maidenhead_rejects_bad_input() {
        assert!(matches!(
            maidenhead_to_geopoint(""),
            Err(GeodesyError::InvalidMaidenhead(_))
        ));
        assert!(matches!(
            maidenhead_to_geopoint("CN8"),
            Err(GeodesyError::InvalidMaidenhead(_))
        ));
        assert!(matches!(
            maidenhead_to_geopoint("ZZ87"),
            Err(GeodesyError::InvalidMaidenhead(_))
        ));
        assert!(matches!(
            maidenhead_to_geopoint("CNAB"),
            Err(GeodesyError::InvalidMaidenhead(_))
        ));
    }

    #[test]
    fn resolve_sample_count_behaviour() {
        assert_eq!(resolve_sample_count(0).unwrap(), DEFAULT_SAMPLE_COUNT);
        assert_eq!(resolve_sample_count(2).unwrap(), 2);
        assert_eq!(resolve_sample_count(64).unwrap(), 64);
        assert_eq!(
            resolve_sample_count(MAX_SAMPLE_COUNT).unwrap(),
            MAX_SAMPLE_COUNT
        );
        assert!(matches!(
            resolve_sample_count(1),
            Err(GeodesyError::SampleCountOutOfRange(1))
        ));
        assert!(matches!(
            resolve_sample_count(MAX_SAMPLE_COUNT + 1),
            Err(GeodesyError::SampleCountOutOfRange(_))
        ));
    }
}
