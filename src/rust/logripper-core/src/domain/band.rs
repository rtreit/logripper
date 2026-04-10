//! Band enum ↔ ADIF string mapping and frequency-to-band derivation.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::proto::logripper::domain::Band;

/// All ADIF 3.1.7 band definitions with frequency ranges.
const BAND_TABLE: &[(&str, Band, f64, f64)] = &[
    ("2190M", Band::Band2190m, 0.1357, 0.1378),
    ("630M", Band::Band630m, 0.472, 0.479),
    ("560M", Band::Band560m, 0.501, 0.504),
    ("160M", Band::Band160m, 1.8, 2.0),
    ("80M", Band::Band80m, 3.5, 4.0),
    ("60M", Band::Band60m, 5.06, 5.45),
    ("40M", Band::Band40m, 7.0, 7.3),
    ("30M", Band::Band30m, 10.1, 10.15),
    ("20M", Band::Band20m, 14.0, 14.35),
    ("17M", Band::Band17m, 18.068, 18.168),
    ("15M", Band::Band15m, 21.0, 21.45),
    ("12M", Band::Band12m, 24.89, 24.99),
    ("10M", Band::Band10m, 28.0, 29.7),
    ("8M", Band::Band8m, 40.0, 45.0),
    ("6M", Band::Band6m, 50.0, 54.0),
    ("5M", Band::Band5m, 54.000_001, 69.9),
    ("4M", Band::Band4m, 70.0, 71.0),
    ("2M", Band::Band2m, 144.0, 148.0),
    ("1.25M", Band::Band125m, 222.0, 225.0),
    ("70CM", Band::Band70cm, 420.0, 450.0),
    ("33CM", Band::Band33cm, 902.0, 928.0),
    ("23CM", Band::Band23cm, 1240.0, 1300.0),
    ("13CM", Band::Band13cm, 2300.0, 2450.0),
    ("9CM", Band::Band9cm, 3300.0, 3500.0),
    ("6CM", Band::Band6cm, 5650.0, 5925.0),
    ("3CM", Band::Band3cm, 10000.0, 10500.0),
    ("1.25CM", Band::Band125cm, 24000.0, 24250.0),
    ("6MM", Band::Band6mm, 47000.0, 47200.0),
    ("4MM", Band::Band4mm, 75500.0, 81000.0),
    ("2.5MM", Band::Band25mm, 119_980.0, 123_000.0),
    ("2MM", Band::Band2mm, 134_000.0, 149_000.0),
    ("1MM", Band::Band1mm, 241_000.0, 250_000.0),
    ("SUBMM", Band::Submm, 300_000.0, 7_500_000.0),
];

/// Map from uppercase ADIF band string → Band enum value, built once at first use.
static ADIF_TO_BAND: LazyLock<HashMap<&'static str, Band>> = LazyLock::new(|| {
    BAND_TABLE
        .iter()
        .map(|(name, band, _, _)| (*name, *band))
        .collect()
});

/// Map from Band enum value → (ADIF string, lower MHz, upper MHz), built once at first use.
static BAND_TO_ENTRY: LazyLock<HashMap<Band, (&'static str, f64, f64)>> = LazyLock::new(|| {
    BAND_TABLE
        .iter()
        .map(|(n, b, lo, hi)| (*b, (*n, *lo, *hi)))
        .collect()
});

/// Parse an ADIF band string (case-insensitive) into a Band enum value.
#[must_use]
pub fn band_from_adif(s: &str) -> Option<Band> {
    let upper = s.to_uppercase();
    ADIF_TO_BAND.get(upper.as_str()).copied()
}

/// Convert a Band enum value to its canonical ADIF string representation.
#[must_use]
pub fn band_to_adif(band: Band) -> Option<&'static str> {
    if band == Band::Unspecified {
        return None;
    }
    BAND_TO_ENTRY.get(&band).map(|(name, _, _)| *name)
}

/// Derive the Band from a frequency in MHz using binary search.
///
/// `BAND_TABLE` is sorted by lower bound, so `partition_point` locates the
/// rightmost candidate in O(log n) rather than scanning all 33 entries.
#[must_use]
pub fn band_from_frequency_mhz(freq_mhz: f64) -> Option<Band> {
    // Find the first entry whose lower bound exceeds freq_mhz.
    let idx = BAND_TABLE.partition_point(|(_, _, lower, _)| *lower <= freq_mhz);
    // The candidate is the last entry with lower ≤ freq_mhz (idx − 1).
    let (_, band, _, upper) = BAND_TABLE.get(idx.checked_sub(1)?)?;
    (freq_mhz <= *upper).then_some(*band)
}

/// Get the frequency range (lower, upper) in MHz for a Band.
#[must_use]
pub fn band_frequency_range_mhz(band: Band) -> Option<(f64, f64)> {
    if band == Band::Unspecified {
        return None;
    }
    BAND_TO_ENTRY.get(&band).map(|(_, lo, hi)| (*lo, *hi))
}

#[cfg(test)]
#[allow(clippy::panic, clippy::unwrap_used, clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn all_bands_round_trip_through_adif_string() {
        for (name, band, _, _) in BAND_TABLE {
            let parsed =
                band_from_adif(name).unwrap_or_else(|| panic!("Failed to parse band: {name}"));
            assert_eq!(parsed, *band, "Band mismatch for {name}");

            let back = band_to_adif(parsed)
                .unwrap_or_else(|| panic!("Failed to convert band back: {name}"));
            assert_eq!(back, *name, "Round-trip mismatch for {name}");
        }
    }

    #[test]
    fn band_from_adif_is_case_insensitive() {
        assert_eq!(band_from_adif("20m"), Some(Band::Band20m));
        assert_eq!(band_from_adif("20M"), Some(Band::Band20m));
        assert_eq!(band_from_adif("70cm"), Some(Band::Band70cm));
        assert_eq!(band_from_adif("70CM"), Some(Band::Band70cm));
        assert_eq!(band_from_adif("submm"), Some(Band::Submm));
    }

    #[test]
    fn band_from_adif_unknown_returns_none() {
        assert_eq!(band_from_adif(""), None);
        assert_eq!(band_from_adif("99M"), None);
        assert_eq!(band_from_adif("bogus"), None);
    }

    #[test]
    fn band_to_adif_unspecified_returns_none() {
        assert_eq!(band_to_adif(Band::Unspecified), None);
    }

    #[test]
    fn frequency_to_band_common_hf() {
        assert_eq!(band_from_frequency_mhz(14.080), Some(Band::Band20m));
        assert_eq!(band_from_frequency_mhz(7.074), Some(Band::Band40m));
        assert_eq!(band_from_frequency_mhz(3.573), Some(Band::Band80m));
        assert_eq!(band_from_frequency_mhz(21.074), Some(Band::Band15m));
        assert_eq!(band_from_frequency_mhz(28.074), Some(Band::Band10m));
    }

    #[test]
    fn frequency_to_band_vhf_uhf() {
        assert_eq!(band_from_frequency_mhz(144.300), Some(Band::Band2m));
        assert_eq!(band_from_frequency_mhz(432.100), Some(Band::Band70cm));
        assert_eq!(band_from_frequency_mhz(1296.000), Some(Band::Band23cm));
    }

    #[test]
    fn frequency_to_band_edge_cases() {
        // Exact lower bound
        assert_eq!(band_from_frequency_mhz(14.0), Some(Band::Band20m));
        // Exact upper bound
        assert_eq!(band_from_frequency_mhz(14.35), Some(Band::Band20m));
        // Between bands
        assert_eq!(band_from_frequency_mhz(15.0), None);
        // Below all bands
        assert_eq!(band_from_frequency_mhz(0.01), None);
    }

    #[test]
    fn frequency_range_round_trips() {
        for (_, band, lower, upper) in BAND_TABLE {
            let range = band_frequency_range_mhz(*band).unwrap();
            assert_eq!(range.0, *lower, "Lower bound mismatch for {band:?}");
            assert_eq!(range.1, *upper, "Upper bound mismatch for {band:?}");
        }
    }

    #[test]
    fn band_count_matches_adif_317() {
        // ADIF 3.1.7 defines 33 bands (2190m through submm)
        assert_eq!(BAND_TABLE.len(), 33);
    }
}
