//! Mode enum ↔ ADIF string mapping.

use std::collections::HashMap;
use std::sync::LazyLock;

use crate::proto::qsoripper::domain::Mode;

/// Mapping between ADIF mode strings and proto Mode enum values.
/// Submodes are NOT enumerated here — they're stored as freeform strings
/// since there are 100+ and they change over time.
const MODE_TABLE: &[(&str, Mode)] = &[
    ("AM", Mode::Am),
    ("ARDOP", Mode::Ardop),
    ("ATV", Mode::Atv),
    ("CHIP", Mode::Chip),
    ("CLO", Mode::Clo),
    ("CONTESTI", Mode::Contesti),
    ("CW", Mode::Cw),
    ("DIGITALVOICE", Mode::Digitalvoice),
    ("DOMINO", Mode::Domino),
    ("DYNAMIC", Mode::Dynamic),
    ("FAX", Mode::Fax),
    ("FM", Mode::Fm),
    ("FSK", Mode::Fsk),
    ("FT8", Mode::Ft8),
    ("HELL", Mode::Hell),
    ("ISCAT", Mode::Iscat),
    ("JT4", Mode::Jt4),
    ("JT9", Mode::Jt9),
    ("JT44", Mode::Jt44),
    ("JT65", Mode::Jt65),
    ("MFSK", Mode::Mfsk),
    ("MTONE", Mode::Mtone),
    ("MSK144", Mode::Msk144),
    ("OFDM", Mode::Ofdm),
    ("OLIVIA", Mode::Olivia),
    ("OPERA", Mode::Opera),
    ("PAC", Mode::Pac),
    ("PAX", Mode::Pax),
    ("PKT", Mode::Pkt),
    ("PSK", Mode::Psk),
    ("Q15", Mode::Q15),
    ("QRA64", Mode::Qra64),
    ("ROS", Mode::Ros),
    ("RTTY", Mode::Rtty),
    ("RTTYM", Mode::Rttym),
    ("SSB", Mode::Ssb),
    ("SSTV", Mode::Sstv),
    ("T10", Mode::T10),
    ("THOR", Mode::Thor),
    ("THRB", Mode::Thrb),
    ("TOR", Mode::Tor),
    ("V4", Mode::V4),
    ("VOI", Mode::Voi),
    ("WINMOR", Mode::Winmor),
    ("WSPR", Mode::Wspr),
];

/// ADIF import-only modes that should be mapped to their replacement.
/// These are deprecated in ADIF 3.1.7 but must be accepted on import.
const IMPORT_ONLY_MODES: &[(&str, Mode, &str)] = &[
    ("C4FM", Mode::Digitalvoice, "C4FM"), // → DIGITALVOICE + submode C4FM
    ("DSTAR", Mode::Digitalvoice, "DSTAR"), // → DIGITALVOICE + submode DSTAR
];

/// Map from uppercase ADIF mode string → Mode enum value, built once at first use.
///
/// Standard modes are inserted first; import-only modes fill in the remaining
/// keys so that a standard name is never overwritten.
static ADIF_TO_MODE: LazyLock<HashMap<&'static str, Mode>> = LazyLock::new(|| {
    let mut map: HashMap<&'static str, Mode> = MODE_TABLE
        .iter()
        .map(|(name, mode)| (*name, *mode))
        .collect();
    for (name, mode, _) in IMPORT_ONLY_MODES {
        map.entry(name).or_insert(*mode);
    }
    map
});

/// Map from Mode enum value → canonical ADIF string, built once at first use.
static MODE_TO_ADIF_MAP: LazyLock<HashMap<Mode, &'static str>> = LazyLock::new(|| {
    MODE_TABLE
        .iter()
        .map(|(name, mode)| (*mode, *name))
        .collect()
});

static SUBMODE_ALIAS_TO_MODE: LazyLock<HashMap<&'static str, (&'static str, Mode)>> =
    LazyLock::new(|| {
        include_str!("../adif/data/submode_aliases.tsv")
            .lines()
            .filter_map(|line| {
                let (submode, mode_name) = line.split_once('\t')?;
                let mode = ADIF_TO_MODE.get(mode_name).copied()?;
                Some((submode, (submode, mode)))
            })
            .collect()
    });

/// Parse an ADIF mode string (case-insensitive) into a Mode enum value.
/// For import-only modes (C4FM, DSTAR), returns the replacement mode.
#[must_use]
pub fn mode_from_adif(s: &str) -> Option<Mode> {
    let upper = s.to_uppercase();
    ADIF_TO_MODE.get(upper.as_str()).copied()
}

/// For import-only modes, returns the submode string that should be set.
/// E.g., "C4FM" → Some("C4FM"), "DSTAR" → Some("DSTAR").
/// For standard modes, returns None.
#[must_use]
pub fn import_only_submode(mode_str: &str) -> Option<&'static str> {
    let upper = mode_str.to_uppercase();
    IMPORT_ONLY_MODES
        .iter()
        .find(|(name, _, _)| *name == upper)
        .map(|(_, _, submode)| *submode)
}

/// Parse an ADIF MODE field into a canonical Mode plus an optional submode alias.
#[must_use]
pub fn normalize_mode_from_adif(s: &str) -> Option<(Mode, Option<&'static str>)> {
    if let Some(mode) = mode_from_adif(s) {
        return Some((mode, import_only_submode(s)));
    }

    let upper = s.to_uppercase();
    SUBMODE_ALIAS_TO_MODE
        .get(upper.as_str())
        .copied()
        .map(|(submode, mode)| (mode, Some(submode)))
}

/// Convert a Mode enum value to its canonical ADIF string representation.
#[must_use]
pub fn mode_to_adif(mode: Mode) -> Option<&'static str> {
    if mode == Mode::Unspecified {
        return None;
    }
    MODE_TO_ADIF_MAP.get(&mode).copied()
}

/// Validate that a submode string is recognized for the given mode.
/// This is a non-exhaustive check for the most common submodes.
/// Returns true if the submode is known to belong to the given mode,
/// or if the mode has no known submode list (permissive).
#[must_use]
pub fn is_known_submode(mode: Mode, submode: &str) -> bool {
    let upper = submode.to_uppercase();
    match mode {
        Mode::Ssb => matches!(upper.as_str(), "USB" | "LSB"),
        Mode::Digitalvoice => matches!(upper.as_str(), "C4FM" | "DMR" | "DSTAR" | "FREEDV" | "M17"),
        Mode::Cw => matches!(upper.as_str(), "PCW"),
        Mode::Ft8 | Mode::Wspr | Mode::Msk144 => false, // no submodes
        _ => true,                                      // permissive for modes with many submodes
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn all_modes_round_trip_through_adif_string() {
        for (name, mode) in MODE_TABLE {
            let parsed = mode_from_adif(name);
            assert_eq!(parsed, Some(*mode), "Mode mismatch for {name}");

            let back = mode_to_adif(*mode);
            assert_eq!(back, Some(*name), "Round-trip mismatch for {name}");
        }
    }

    #[test]
    fn mode_from_adif_is_case_insensitive() {
        assert_eq!(mode_from_adif("ssb"), Some(Mode::Ssb));
        assert_eq!(mode_from_adif("SSB"), Some(Mode::Ssb));
        assert_eq!(mode_from_adif("Ssb"), Some(Mode::Ssb));
        assert_eq!(mode_from_adif("ft8"), Some(Mode::Ft8));
        assert_eq!(mode_from_adif("FT8"), Some(Mode::Ft8));
        assert_eq!(mode_from_adif("cw"), Some(Mode::Cw));
    }

    #[test]
    fn mode_from_adif_unknown_returns_none() {
        assert_eq!(mode_from_adif(""), None);
        assert_eq!(mode_from_adif("BOGUS"), None);
        assert_eq!(mode_from_adif("USB"), None); // USB is a submode, not a mode
    }

    #[test]
    fn mode_to_adif_unspecified_returns_none() {
        assert_eq!(mode_to_adif(Mode::Unspecified), None);
    }

    #[test]
    fn import_only_c4fm_maps_to_digitalvoice() {
        assert_eq!(mode_from_adif("C4FM"), Some(Mode::Digitalvoice));
        assert_eq!(import_only_submode("C4FM"), Some("C4FM"));
    }

    #[test]
    fn import_only_dstar_maps_to_digitalvoice() {
        assert_eq!(mode_from_adif("DSTAR"), Some(Mode::Digitalvoice));
        assert_eq!(import_only_submode("DSTAR"), Some("DSTAR"));
    }

    #[test]
    fn standard_mode_has_no_import_only_submode() {
        assert_eq!(import_only_submode("SSB"), None);
        assert_eq!(import_only_submode("FT8"), None);
    }

    #[test]
    fn normalize_mode_from_known_submode_alias() {
        assert_eq!(
            normalize_mode_from_adif("PSK31"),
            Some((Mode::Psk, Some("PSK31")))
        );
        assert_eq!(
            normalize_mode_from_adif("usb"),
            Some((Mode::Ssb, Some("USB")))
        );
    }

    #[test]
    fn ssb_submodes_recognized() {
        assert!(is_known_submode(Mode::Ssb, "USB"));
        assert!(is_known_submode(Mode::Ssb, "LSB"));
        assert!(!is_known_submode(Mode::Ssb, "PSK31"));
    }

    #[test]
    fn digitalvoice_submodes_recognized() {
        assert!(is_known_submode(Mode::Digitalvoice, "C4FM"));
        assert!(is_known_submode(Mode::Digitalvoice, "DMR"));
        assert!(is_known_submode(Mode::Digitalvoice, "DSTAR"));
        assert!(is_known_submode(Mode::Digitalvoice, "FREEDV"));
        assert!(is_known_submode(Mode::Digitalvoice, "M17"));
    }

    #[test]
    fn mode_count_matches_adif_317() {
        // ADIF 3.1.7 defines 45 standard modes (excluding import-only)
        assert_eq!(MODE_TABLE.len(), 45);
    }
}
