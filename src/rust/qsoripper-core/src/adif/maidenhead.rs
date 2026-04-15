//! Maidenhead grid-square ↔ coordinate conversion.

/// Convert a Maidenhead grid square locator to (latitude, longitude) center
/// coordinates.  Supports 4, 6, and 8 character locators.
pub(crate) fn grid_to_latlon(grid: &str) -> Option<(f64, f64)> {
    let grid = grid.trim();
    let grid_len = grid.len();
    if grid_len < 4 || grid_len % 2 != 0 || grid_len > 8 {
        return None;
    }

    let bytes = grid.as_bytes();

    // Field (first pair): A-R
    let field_lon = letter_value(*bytes.first()?, b'A', b'R')?;
    let field_lat = letter_value(*bytes.get(1)?, b'A', b'R')?;

    // Square (second pair): 0-9
    let square_lon = digit_value(*bytes.get(2)?)?;
    let square_lat = digit_value(*bytes.get(3)?)?;

    let mut longitude = f64::from(field_lon) * 20.0 - 180.0 + f64::from(square_lon) * 2.0;
    let mut latitude = f64::from(field_lat) * 10.0 - 90.0 + f64::from(square_lat);

    let mut lon_step = 2.0;
    let mut lat_step = 1.0;

    if grid_len >= 6 {
        // Subsquare (third pair): a-x (case insensitive)
        let sub_lon = letter_value(bytes.get(4)?.to_ascii_uppercase(), b'A', b'X')?;
        let sub_lat = letter_value(bytes.get(5)?.to_ascii_uppercase(), b'A', b'X')?;
        lon_step = 2.0 / 24.0;
        lat_step = 1.0 / 24.0;
        longitude += f64::from(sub_lon) * lon_step;
        latitude += f64::from(sub_lat) * lat_step;
    }

    if grid_len >= 8 {
        // Extended square (fourth pair): 0-9
        let ext_lon = digit_value(*bytes.get(6)?)?;
        let ext_lat = digit_value(*bytes.get(7)?)?;
        let ext_lon_step = lon_step / 10.0;
        let ext_lat_step = lat_step / 10.0;
        longitude += f64::from(ext_lon) * ext_lon_step;
        latitude += f64::from(ext_lat) * ext_lat_step;
        lon_step = ext_lon_step;
        lat_step = ext_lat_step;
    }

    // Return center of the grid cell
    longitude += lon_step / 2.0;
    latitude += lat_step / 2.0;

    Some((latitude, longitude))
}

fn letter_value(byte: u8, min: u8, max: u8) -> Option<u8> {
    let upper = byte.to_ascii_uppercase();
    if (min..=max).contains(&upper) {
        Some(upper - min)
    } else {
        None
    }
}

fn digit_value(byte: u8) -> Option<u8> {
    byte.is_ascii_digit().then_some(byte - b'0')
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::grid_to_latlon;

    fn approx(actual: f64, expected: f64, tolerance: f64) -> bool {
        (actual - expected).abs() < tolerance
    }

    #[test]
    fn cn87_returns_western_washington() {
        let (lat, lon) = grid_to_latlon("CN87").unwrap();
        assert!(approx(lat, 47.5, 1.0), "lat={lat}");
        assert!(approx(lon, -123.0, 2.0), "lon={lon}");
    }

    #[test]
    fn fn31_returns_connecticut_area() {
        let (lat, lon) = grid_to_latlon("FN31").unwrap();
        assert!(approx(lat, 41.5, 1.0), "lat={lat}");
        assert!(approx(lon, -73.0, 2.0), "lon={lon}");
    }

    #[test]
    fn jo22_returns_netherlands() {
        let (lat, lon) = grid_to_latlon("JO22").unwrap();
        assert!(approx(lat, 52.5, 1.0), "lat={lat}");
        assert!(approx(lon, 5.0, 2.0), "lon={lon}");
    }

    #[test]
    fn cn87xr_six_char_more_precise() {
        let (lat, lon) = grid_to_latlon("CN87xr").unwrap();
        // Must fall within the CN87 4-char bounding box
        assert!((47.0..=48.0).contains(&lat), "lat={lat}");
        assert!((-124.0..=-122.0).contains(&lon), "lon={lon}");
    }

    #[test]
    fn eight_char_locator() {
        let (lat, lon) = grid_to_latlon("CN87xr50").unwrap();
        assert!((47.0..=48.0).contains(&lat), "lat={lat}");
        assert!((-124.0..=-122.0).contains(&lon), "lon={lon}");
    }

    #[test]
    fn case_insensitive_subsquare() {
        let lower = grid_to_latlon("cn87xr").unwrap();
        let upper = grid_to_latlon("CN87XR").unwrap();
        assert!(approx(lower.0, upper.0, 0.001));
        assert!(approx(lower.1, upper.1, 0.001));
    }

    #[test]
    fn empty_string_returns_none() {
        assert!(grid_to_latlon("").is_none());
    }

    #[test]
    fn two_chars_returns_none() {
        assert!(grid_to_latlon("AB").is_none());
    }

    #[test]
    fn three_chars_returns_none() {
        assert!(grid_to_latlon("CN8").is_none());
    }

    #[test]
    fn field_out_of_range_returns_none() {
        // Z is beyond R for the field pair
        assert!(grid_to_latlon("ZZ99").is_none());
    }

    #[test]
    fn aa00_bottom_left_corner() {
        let (lat, lon) = grid_to_latlon("AA00").unwrap();
        assert!((-90.0..=90.0).contains(&lat), "lat={lat}");
        assert!((-180.0..=180.0).contains(&lon), "lon={lon}");
        assert!(approx(lat, -89.5, 1.0), "lat={lat}");
        assert!(approx(lon, -179.0, 2.0), "lon={lon}");
    }

    #[test]
    fn rr99_top_right_corner() {
        let (lat, lon) = grid_to_latlon("RR99").unwrap();
        assert!((-90.0..=90.0).contains(&lat), "lat={lat}");
        assert!((-180.0..=180.0).contains(&lon), "lon={lon}");
        assert!(approx(lat, 89.5, 1.0), "lat={lat}");
        assert!(approx(lon, 179.0, 2.0), "lon={lon}");
    }

    #[test]
    fn subsquare_y_out_of_range_returns_none() {
        // 'Y' is beyond 'X' for the subsquare pair
        assert!(grid_to_latlon("CN87ya").is_none());
    }
}
