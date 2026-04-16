//! Callsign parser for slash-modified amateur radio callsigns.
//!
//! Derives structured components from callsigns like `AE7XI/P`, `EA8/AE7XI`,
//! `AE7XI/5`, and `DL/AE7XI/P` without modifying the original entered value.
//!
//! # Parsing rules
//!
//! | Input         | base      | modifier_text | modifier_kind   | prefix_override | ambiguity   |
//! |---------------|-----------|---------------|-----------------|-----------------|-------------|
//! | `AE7XI`       | `AE7XI`   | None          | None            | None            | Standard    |
//! | `AE7XI/P`     | `AE7XI`   | "P"           | Portable        | None            | Standard    |
//! | `AE7XI/M`     | `AE7XI`   | "M"           | Mobile          | None            | Standard    |
//! | `AE7XI/MM`    | `AE7XI`   | "MM"          | MaritimeMobile  | None            | Standard    |
//! | `AE7XI/AM`    | `AE7XI`   | "AM"          | AeronauticalMobile | None         | Standard    |
//! | `AE7XI/5`     | `AE7XI`   | "5"           | CallArea        | None            | Standard    |
//! | `EA8/AE7XI`   | `AE7XI`   | "EA8"         | PrefixOverride  | Some("EA8")     | Standard    |
//! | `DL/AE7XI/P`  | `AE7XI`   | "DL"          | PrefixOverride  | Some("DL")      | Ambiguous   |

use crate::proto::qsoripper::domain::{
    CallsignAmbiguity as ProtoCallsignAmbiguity, CallsignRecord, ModifierKind as ProtoModifierKind,
};

/// The operating modifier kind derived from a slash-modified callsign.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModifierKind {
    /// Portable operation (e.g. `/P`).
    Portable,
    /// Mobile operation (e.g. `/M`).
    Mobile,
    /// Maritime mobile operation (e.g. `/MM`).
    MaritimeMobile,
    /// Aeronautical mobile operation (e.g. `/AM`).
    AeronauticalMobile,
    /// Operating from a different call area (e.g. `/5`).
    CallArea,
    /// Foreign prefix override (e.g. `EA8/AE7XI`).
    PrefixOverride,
    /// Unrecognized modifier (e.g. `/QRP`).
    Other,
}

/// Indicates how unambiguously the callsign was parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallsignAmbiguity {
    /// Callsign parsed without ambiguity.
    Standard,
    /// Multiple valid interpretations exist (e.g. `DL/AE7XI/P`).
    Ambiguous,
    /// Does not conform to known patterns.
    NonStandard,
}

/// Derived components of a slash-modified callsign.
///
/// The original callsign string is never modified; all fields are derived
/// for lookup enrichment and display purposes only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCallsign {
    /// The base callsign without modifiers (e.g. `AE7XI` from `AE7XI/P`).
    pub base_callsign: String,
    /// The raw modifier token, if any (e.g. `"P"`, `"EA8"`, `"DL"`).
    pub modifier_text: Option<String>,
    /// The interpreted kind of modifier.
    pub modifier_kind: Option<ModifierKind>,
    /// Populated for [`ModifierKind::PrefixOverride`] forms like `EA8/AE7XI`.
    pub prefix_override: Option<String>,
    /// Optional operating entity hint (informational; caller may populate).
    pub operating_entity_hint: Option<String>,
    /// How unambiguously the callsign was parsed.
    pub ambiguity: CallsignAmbiguity,
}

/// Well-known suffix modifier tokens and their kinds.
///
/// Checked against the normalized (uppercase) input segment.
const KNOWN_SUFFIXES: &[(&str, ModifierKind)] = &[
    ("P", ModifierKind::Portable),
    ("M", ModifierKind::Mobile),
    ("MM", ModifierKind::MaritimeMobile),
    ("AM", ModifierKind::AeronauticalMobile),
];

/// Parse a normalized callsign into its derived components.
///
/// The input must already be normalized (uppercase, trimmed). The returned
/// [`ParsedCallsign`] carries the original string's components without
/// modifying or replacing the original.
#[must_use]
pub fn parse_callsign(normalized: &str) -> ParsedCallsign {
    let segments: Vec<&str> = normalized.split('/').collect();

    match segments.as_slice() {
        [only] => ParsedCallsign {
            base_callsign: (*only).to_string(),
            modifier_text: None,
            modifier_kind: None,
            prefix_override: None,
            operating_entity_hint: None,
            ambiguity: CallsignAmbiguity::Standard,
        },

        [left, right] => parse_two_segment(left, right),

        [left, middle, _right] => ParsedCallsign {
            // Three-segment forms (e.g. DL/AE7XI/P): the middle segment is
            // the base call, the first segment is the prefix override, and
            // the trailing modifier is noted via Ambiguous ambiguity.
            base_callsign: (*middle).to_string(),
            modifier_text: Some((*left).to_string()),
            modifier_kind: Some(ModifierKind::PrefixOverride),
            prefix_override: Some((*left).to_string()),
            operating_entity_hint: None,
            ambiguity: CallsignAmbiguity::Ambiguous,
        },

        _ => ParsedCallsign {
            // Four or more segments — treat as non-standard; preserve raw.
            base_callsign: normalized.to_string(),
            modifier_text: None,
            modifier_kind: None,
            prefix_override: None,
            operating_entity_hint: None,
            ambiguity: CallsignAmbiguity::NonStandard,
        },
    }
}

fn parse_two_segment(left: &str, right: &str) -> ParsedCallsign {
    // Check well-known suffix modifiers (P, M, MM, AM).
    for (token, kind) in KNOWN_SUFFIXES {
        if right == *token {
            return ParsedCallsign {
                base_callsign: left.to_string(),
                modifier_text: Some(right.to_string()),
                modifier_kind: Some(kind.clone()),
                prefix_override: None,
                operating_entity_hint: None,
                ambiguity: CallsignAmbiguity::Standard,
            };
        }
    }

    // Single digit → call area (e.g. AE7XI/5, K7DBG/0).
    if right.len() == 1 && right.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        return ParsedCallsign {
            base_callsign: left.to_string(),
            modifier_text: Some(right.to_string()),
            modifier_kind: Some(ModifierKind::CallArea),
            prefix_override: None,
            operating_entity_hint: None,
            ambiguity: CallsignAmbiguity::Standard,
        };
    }

    // Prefix override: left looks like a DXCC prefix while right looks like
    // a full callsign (contains at least one digit and one letter).
    if looks_like_prefix(left) && looks_like_full_callsign(right) {
        return ParsedCallsign {
            base_callsign: right.to_string(),
            modifier_text: Some(left.to_string()),
            modifier_kind: Some(ModifierKind::PrefixOverride),
            prefix_override: Some(left.to_string()),
            operating_entity_hint: None,
            ambiguity: CallsignAmbiguity::Standard,
        };
    }

    // Non-standard or unrecognized modifier.
    ParsedCallsign {
        base_callsign: left.to_string(),
        modifier_text: Some(right.to_string()),
        modifier_kind: Some(ModifierKind::Other),
        prefix_override: None,
        operating_entity_hint: None,
        ambiguity: CallsignAmbiguity::NonStandard,
    }
}

/// Returns `true` if the segment looks like a DXCC prefix rather than a full
/// callsign: starts with a letter, at most 4 characters, all alphanumeric.
fn looks_like_prefix(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 4
        && segment.starts_with(|ch: char| ch.is_ascii_alphabetic())
        && segment.chars().all(|ch| ch.is_ascii_alphanumeric())
}

/// Returns `true` if the segment looks like a full (base) callsign by
/// containing at least one digit and at least one letter.
fn looks_like_full_callsign(segment: &str) -> bool {
    segment.chars().any(|ch| ch.is_ascii_digit())
        && segment.chars().any(|ch| ch.is_ascii_alphabetic())
}

/// Map a Rust [`ModifierKind`] to its proto i32 discriminant.
fn proto_modifier_kind(kind: &ModifierKind) -> i32 {
    match kind {
        ModifierKind::Portable => ProtoModifierKind::Portable as i32,
        ModifierKind::Mobile => ProtoModifierKind::Mobile as i32,
        ModifierKind::MaritimeMobile => ProtoModifierKind::MaritimeMobile as i32,
        ModifierKind::AeronauticalMobile => ProtoModifierKind::AeronauticalMobile as i32,
        ModifierKind::CallArea => ProtoModifierKind::CallArea as i32,
        ModifierKind::PrefixOverride => ProtoModifierKind::PrefixOverride as i32,
        ModifierKind::Other => ProtoModifierKind::Other as i32,
    }
}

/// Map a Rust [`CallsignAmbiguity`] to its proto i32 discriminant.
fn proto_callsign_ambiguity(ambiguity: &CallsignAmbiguity) -> i32 {
    match ambiguity {
        CallsignAmbiguity::Standard => ProtoCallsignAmbiguity::Standard as i32,
        CallsignAmbiguity::Ambiguous => ProtoCallsignAmbiguity::Ambiguous as i32,
        CallsignAmbiguity::NonStandard => ProtoCallsignAmbiguity::NonStandard as i32,
    }
}

/// Stamp a [`CallsignRecord`] with modifier fields derived from a parsed
/// callsign.  The `base_callsign` field is only set when it differs from the
/// primary `callsign` field (i.e. when a modifier was present).
pub fn annotate_record(record: &mut CallsignRecord, parsed: &ParsedCallsign) {
    if parsed.modifier_kind.is_none() {
        // No modifier — nothing to annotate.
        return;
    }

    record.base_callsign = Some(parsed.base_callsign.clone());
    record.modifier_text.clone_from(&parsed.modifier_text);
    record.modifier_kind = parsed
        .modifier_kind
        .as_ref()
        .map_or(Some(0), |kind| Some(proto_modifier_kind(kind)));
    record
        .prefix_override_callsign
        .clone_from(&parsed.prefix_override);
    record
        .operating_entity_hint
        .clone_from(&parsed.operating_entity_hint);
    record.callsign_ambiguity = Some(proto_callsign_ambiguity(&parsed.ambiguity));
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;
    use crate::domain::lookup::normalize_callsign;

    #[test]
    fn bare_callsign_no_modifier() {
        let parsed = parse_callsign("W1AW");
        assert_eq!(parsed.base_callsign, "W1AW");
        assert_eq!(parsed.modifier_text, None);
        assert_eq!(parsed.modifier_kind, None);
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn portable_suffix() {
        let parsed = parse_callsign("AE7XI/P");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_text.as_deref(), Some("P"));
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::Portable));
        assert_eq!(parsed.prefix_override, None);
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn mobile_suffix() {
        let parsed = parse_callsign("AE7XI/M");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::Mobile));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn maritime_mobile_suffix() {
        let parsed = parse_callsign("AE7XI/MM");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::MaritimeMobile));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn aeronautical_mobile_suffix() {
        let parsed = parse_callsign("AE7XI/AM");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::AeronauticalMobile));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn call_area_suffix() {
        let parsed = parse_callsign("AE7XI/5");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_text.as_deref(), Some("5"));
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::CallArea));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn call_area_zero() {
        let parsed = parse_callsign("K7DBG/0");
        assert_eq!(parsed.base_callsign, "K7DBG");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::CallArea));
    }

    #[test]
    fn prefix_override_ea8() {
        let parsed = parse_callsign("EA8/AE7XI");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_text.as_deref(), Some("EA8"));
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::PrefixOverride));
        assert_eq!(parsed.prefix_override.as_deref(), Some("EA8"));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Standard);
    }

    #[test]
    fn triple_form_dl_ae7xi_p() {
        let parsed = parse_callsign("DL/AE7XI/P");
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_text.as_deref(), Some("DL"));
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::PrefixOverride));
        assert_eq!(parsed.prefix_override.as_deref(), Some("DL"));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::Ambiguous);
    }

    #[test]
    fn vk9_prefix_override() {
        let parsed = parse_callsign("VK9/W1AW");
        assert_eq!(parsed.base_callsign, "W1AW");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::PrefixOverride));
        assert_eq!(parsed.prefix_override.as_deref(), Some("VK9"));
    }

    #[test]
    fn qrp_suffix_is_other() {
        let parsed = parse_callsign("W1AW/QRP");
        assert_eq!(parsed.base_callsign, "W1AW");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::Other));
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::NonStandard);
    }

    #[test]
    fn four_segments_is_non_standard() {
        let parsed = parse_callsign("A/B/C/D");
        assert_eq!(parsed.base_callsign, "A/B/C/D");
        assert_eq!(parsed.modifier_kind, None);
        assert_eq!(parsed.ambiguity, CallsignAmbiguity::NonStandard);
    }

    #[test]
    fn normalize_then_parse_preserves_slash() {
        let normalized = normalize_callsign("ae7xi/p");
        assert_eq!(normalized, "AE7XI/P");
        let parsed = parse_callsign(&normalized);
        assert_eq!(parsed.base_callsign, "AE7XI");
        assert_eq!(parsed.modifier_kind, Some(ModifierKind::Portable));
    }

    #[test]
    fn annotate_record_stamps_modifier_fields_when_present() {
        let parsed = parse_callsign("AE7XI/P");
        let mut record = CallsignRecord {
            callsign: "AE7XI/P".to_string(),
            ..Default::default()
        };
        annotate_record(&mut record, &parsed);
        assert_eq!(record.base_callsign.as_deref(), Some("AE7XI"));
        assert_eq!(record.modifier_text.as_deref(), Some("P"));
        assert_eq!(
            record.modifier_kind,
            Some(ProtoModifierKind::Portable as i32)
        );
        assert_eq!(record.prefix_override_callsign, None);
        assert_eq!(
            record.callsign_ambiguity,
            Some(ProtoCallsignAmbiguity::Standard as i32)
        );
    }

    #[test]
    fn annotate_record_skips_bare_callsign() {
        let parsed = parse_callsign("W1AW");
        let mut record = CallsignRecord {
            callsign: "W1AW".to_string(),
            ..Default::default()
        };
        annotate_record(&mut record, &parsed);
        assert_eq!(record.base_callsign, None);
        assert_eq!(record.modifier_text, None);
        assert_eq!(record.modifier_kind, None);
    }
}
