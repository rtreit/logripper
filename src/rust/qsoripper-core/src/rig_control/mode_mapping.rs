//! Hamlib mode string to proto Mode/submode mapping.

use crate::proto::qsoripper::domain::Mode;

/// Result of mapping a Hamlib mode string to project-owned types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeMapping {
    /// The normalized project-owned mode.
    pub mode: Mode,
    /// Optional submode (e.g., `"USB"` for SSB).
    pub submode: Option<String>,
}

/// Map a Hamlib/rigctld mode string to a project-owned [`Mode`] and optional submode.
///
/// The mapping is case-insensitive. Unknown modes map to [`Mode::Unspecified`].
#[must_use]
pub fn hamlib_mode_to_proto(raw: &str) -> ModeMapping {
    match raw.to_uppercase().as_str() {
        "USB" => ModeMapping {
            mode: Mode::Ssb,
            submode: Some("USB".to_string()),
        },
        "LSB" => ModeMapping {
            mode: Mode::Ssb,
            submode: Some("LSB".to_string()),
        },
        "CW" => ModeMapping {
            mode: Mode::Cw,
            submode: None,
        },
        "CWR" => ModeMapping {
            mode: Mode::Cw,
            submode: Some("CWR".to_string()),
        },
        "AM" => ModeMapping {
            mode: Mode::Am,
            submode: None,
        },
        "FM" | "WFM" => ModeMapping {
            mode: Mode::Fm,
            submode: None,
        },
        "RTTY" => ModeMapping {
            mode: Mode::Rtty,
            submode: None,
        },
        "RTTYR" => ModeMapping {
            mode: Mode::Rtty,
            submode: Some("RTTYR".to_string()),
        },
        // PKTUSB/PKTLSB: digital packet modes (FT8, JS8Call, etc.)
        // Map to PKT, not SSB, to avoid wrong RST defaults.
        "PKTUSB" => ModeMapping {
            mode: Mode::Pkt,
            submode: Some("PKTUSB".to_string()),
        },
        "PKTLSB" => ModeMapping {
            mode: Mode::Pkt,
            submode: Some("PKTLSB".to_string()),
        },
        "PKTFM" => ModeMapping {
            mode: Mode::Pkt,
            submode: Some("PKTFM".to_string()),
        },
        "FT8" => ModeMapping {
            mode: Mode::Ft8,
            submode: None,
        },
        "PSK" | "PSK31" => ModeMapping {
            mode: Mode::Psk,
            submode: None,
        },
        _ => ModeMapping {
            mode: Mode::Unspecified,
            submode: None,
        },
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn ssb_modes() {
        let usb = hamlib_mode_to_proto("USB");
        assert_eq!(Mode::Ssb, usb.mode);
        assert_eq!(Some("USB".to_string()), usb.submode);

        let lsb = hamlib_mode_to_proto("LSB");
        assert_eq!(Mode::Ssb, lsb.mode);
        assert_eq!(Some("LSB".to_string()), lsb.submode);
    }

    #[test]
    fn cw_modes() {
        let cw = hamlib_mode_to_proto("CW");
        assert_eq!(Mode::Cw, cw.mode);
        assert_eq!(None, cw.submode);

        let cwr = hamlib_mode_to_proto("CWR");
        assert_eq!(Mode::Cw, cwr.mode);
        assert_eq!(Some("CWR".to_string()), cwr.submode);
    }

    #[test]
    fn pktusb_maps_to_pkt() {
        let result = hamlib_mode_to_proto("PKTUSB");
        assert_eq!(Mode::Pkt, result.mode);
        assert_eq!(Some("PKTUSB".to_string()), result.submode);
    }

    #[test]
    fn pktlsb_maps_to_pkt() {
        let result = hamlib_mode_to_proto("PKTLSB");
        assert_eq!(Mode::Pkt, result.mode);
        assert_eq!(Some("PKTLSB".to_string()), result.submode);
    }

    #[test]
    fn fm_modes() {
        assert_eq!(Mode::Fm, hamlib_mode_to_proto("FM").mode);
        assert_eq!(Mode::Fm, hamlib_mode_to_proto("WFM").mode);
    }

    #[test]
    fn rtty_modes() {
        assert_eq!(Mode::Rtty, hamlib_mode_to_proto("RTTY").mode);

        let rttyr = hamlib_mode_to_proto("RTTYR");
        assert_eq!(Mode::Rtty, rttyr.mode);
        assert_eq!(Some("RTTYR".to_string()), rttyr.submode);
    }

    #[test]
    fn case_insensitive() {
        assert_eq!(Mode::Ssb, hamlib_mode_to_proto("usb").mode);
        assert_eq!(Mode::Cw, hamlib_mode_to_proto("cw").mode);
        assert_eq!(Mode::Pkt, hamlib_mode_to_proto("pktusb").mode);
    }

    #[test]
    fn unknown_mode() {
        let result = hamlib_mode_to_proto("SOMETHING_NEW");
        assert_eq!(Mode::Unspecified, result.mode);
        assert_eq!(None, result.submode);
    }

    #[test]
    fn ft8_mode() {
        let result = hamlib_mode_to_proto("FT8");
        assert_eq!(Mode::Ft8, result.mode);
        assert_eq!(None, result.submode);
    }
}
