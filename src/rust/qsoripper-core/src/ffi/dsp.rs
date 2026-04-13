#![allow(unsafe_code)]

extern "C" {
    fn lr_dsp_version() -> i32;
    fn lr_dsp_hz_to_khz(freq_hz: u64) -> u64;
    fn lr_dsp_moving_average(samples: *const f64, count: usize) -> f64;
}

/// Return the version number exposed by the DSP library.
#[must_use]
pub fn dsp_version() -> i32 {
    unsafe { lr_dsp_version() }
}

/// Convert a frequency in Hz to kHz using the DSP helper implementation.
#[must_use]
pub fn hz_to_khz(freq_hz: u64) -> u64 {
    unsafe { lr_dsp_hz_to_khz(freq_hz) }
}

/// Compute the arithmetic mean for the provided sample slice.
#[must_use]
pub fn moving_average(samples: &[f64]) -> f64 {
    unsafe { lr_dsp_moving_average(samples.as_ptr(), samples.len()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_returns_expected() {
        assert_eq!(dsp_version(), 1);
    }

    #[test]
    fn hz_to_khz_rounds_correctly() {
        assert_eq!(hz_to_khz(14_074_000), 14_074);
        assert_eq!(hz_to_khz(14_074_499), 14_074);
        assert_eq!(hz_to_khz(14_074_500), 14_075);
        assert_eq!(hz_to_khz(0), 0);
    }

    #[test]
    fn moving_average_basic() {
        let samples = [1.0, 2.0, 3.0, 4.0, 5.0];
        let avg = moving_average(&samples);
        assert!((avg - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn moving_average_empty_slice() {
        let avg = moving_average(&[]);
        assert!((avg - 0.0).abs() < f64::EPSILON);
    }
}
