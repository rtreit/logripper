//! Frequency-to-band mapping using IARU band plan ranges.

use crate::proto::qsoripper::domain::Band;

/// Map a frequency in Hz to the corresponding amateur radio band.
///
/// Returns [`Band::Unspecified`] if the frequency does not fall within any
/// recognized amateur band.
#[must_use]
pub fn frequency_hz_to_band(hz: u64) -> Band {
    match hz {
        // 2200m: 135.7–137.8 kHz
        135_700..=137_800 => Band::Band2190m,
        // 630m: 472–479 kHz
        472_000..=479_000 => Band::Band630m,
        // 160m: 1.8–2.0 MHz
        1_800_000..=2_000_000 => Band::Band160m,
        // 80m: 3.5–4.0 MHz
        3_500_000..=4_000_000 => Band::Band80m,
        // 60m: 5.06–5.45 MHz (channelized in most countries)
        5_060_000..=5_450_000 => Band::Band60m,
        // 40m: 7.0–7.3 MHz
        7_000_000..=7_300_000 => Band::Band40m,
        // 30m: 10.1–10.15 MHz
        10_100_000..=10_150_000 => Band::Band30m,
        // 20m: 14.0–14.35 MHz
        14_000_000..=14_350_000 => Band::Band20m,
        // 17m: 18.068–18.168 MHz
        18_068_000..=18_168_000 => Band::Band17m,
        // 15m: 21.0–21.45 MHz
        21_000_000..=21_450_000 => Band::Band15m,
        // 12m: 24.89–24.99 MHz
        24_890_000..=24_990_000 => Band::Band12m,
        // 10m: 28.0–29.7 MHz
        28_000_000..=29_700_000 => Band::Band10m,
        // 6m: 50.0–54.0 MHz
        50_000_000..=54_000_000 => Band::Band6m,
        // 2m: 144.0–148.0 MHz
        144_000_000..=148_000_000 => Band::Band2m,
        // 1.25m: 219–225 MHz
        219_000_000..=225_000_000 => Band::Band125m,
        // 70cm: 420–450 MHz
        420_000_000..=450_000_000 => Band::Band70cm,
        // 33cm: 902–928 MHz
        902_000_000..=928_000_000 => Band::Band33cm,
        // 23cm: 1240–1300 MHz
        1_240_000_000..=1_300_000_000 => Band::Band23cm,
        _ => Band::Unspecified,
    }
}

/// Convert a frequency in Hz to kHz, rounding to the nearest kHz.
#[must_use]
pub fn frequency_hz_to_khz(hz: u64) -> u64 {
    (hz + 500) / 1_000
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn common_hf_frequencies() {
        assert_eq!(Band::Band160m, frequency_hz_to_band(1_840_000));
        assert_eq!(Band::Band80m, frequency_hz_to_band(3_573_000));
        assert_eq!(Band::Band60m, frequency_hz_to_band(5_357_000));
        assert_eq!(Band::Band40m, frequency_hz_to_band(7_074_000));
        assert_eq!(Band::Band30m, frequency_hz_to_band(10_136_000));
        assert_eq!(Band::Band20m, frequency_hz_to_band(14_074_000));
        assert_eq!(Band::Band17m, frequency_hz_to_band(18_100_000));
        assert_eq!(Band::Band15m, frequency_hz_to_band(21_074_000));
        assert_eq!(Band::Band12m, frequency_hz_to_band(24_915_000));
        assert_eq!(Band::Band10m, frequency_hz_to_band(28_074_000));
    }

    #[test]
    fn vhf_uhf_frequencies() {
        assert_eq!(Band::Band6m, frequency_hz_to_band(50_313_000));
        assert_eq!(Band::Band2m, frequency_hz_to_band(144_174_000));
        assert_eq!(Band::Band70cm, frequency_hz_to_band(432_065_000));
    }

    #[test]
    fn band_edge_lower_boundary() {
        assert_eq!(Band::Band20m, frequency_hz_to_band(14_000_000));
        assert_eq!(Band::Band40m, frequency_hz_to_band(7_000_000));
        assert_eq!(Band::Band80m, frequency_hz_to_band(3_500_000));
    }

    #[test]
    fn band_edge_upper_boundary() {
        assert_eq!(Band::Band20m, frequency_hz_to_band(14_350_000));
        assert_eq!(Band::Band40m, frequency_hz_to_band(7_300_000));
        assert_eq!(Band::Band10m, frequency_hz_to_band(29_700_000));
    }

    #[test]
    fn out_of_range_returns_unspecified() {
        assert_eq!(Band::Unspecified, frequency_hz_to_band(0));
        assert_eq!(Band::Unspecified, frequency_hz_to_band(100_000));
        assert_eq!(Band::Unspecified, frequency_hz_to_band(13_999_999));
        assert_eq!(Band::Unspecified, frequency_hz_to_band(14_350_001));
    }

    #[test]
    fn low_frequency_bands() {
        assert_eq!(Band::Band2190m, frequency_hz_to_band(136_000));
        assert_eq!(Band::Band630m, frequency_hz_to_band(475_000));
    }

    #[test]
    fn microwave_bands() {
        assert_eq!(Band::Band33cm, frequency_hz_to_band(903_000_000));
        assert_eq!(Band::Band23cm, frequency_hz_to_band(1_296_000_000));
    }

    #[test]
    fn hz_to_khz_rounds() {
        assert_eq!(14_074, frequency_hz_to_khz(14_074_000));
        assert_eq!(14_074, frequency_hz_to_khz(14_074_499));
        assert_eq!(14_075, frequency_hz_to_khz(14_074_500));
        assert_eq!(7_074, frequency_hz_to_khz(7_074_000));
        assert_eq!(0, frequency_hz_to_khz(0));
    }
}
