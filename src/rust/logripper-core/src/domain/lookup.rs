//! Lookup-related helpers for callsign normalization and placeholder responses.

use crate::proto::logripper::domain::{CallsignRecord, LookupResult, LookupState};

/// Normalize a callsign for lookup and cache-key use.
///
/// Blank input currently falls back to the developer placeholder callsign used by
/// the stub lookup surface.
#[must_use]
pub fn normalize_callsign(callsign: &str) -> String {
    let trimmed = callsign.trim();
    if trimmed.is_empty() {
        "K7DBG".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

/// Build the current developer placeholder error result for a callsign lookup.
#[must_use]
pub fn placeholder_lookup_error(callsign: &str) -> LookupResult {
    let normalized_callsign = normalize_callsign(callsign);

    LookupResult {
        state: LookupState::Error as i32,
        record: Some(CallsignRecord {
            callsign: normalized_callsign.clone(),
            cross_ref: normalized_callsign.clone(),
            aliases: Vec::new(),
            previous_call: String::new(),
            dxcc_entity_id: 0,
            first_name: "Developer".into(),
            last_name: "Placeholder".into(),
            nickname: None,
            formatted_name: Some("Developer Placeholder".into()),
            attention: None,
            addr1: None,
            addr2: Some("Local server stub".into()),
            state: None,
            zip: None,
            country: Some("Unavailable".into()),
            country_code: None,
            latitude: None,
            longitude: None,
            grid_square: None,
            county: None,
            fips: None,
            geo_source: 7,
            license_class: None,
            effective_date: None,
            expiration_date: None,
            license_codes: None,
            email: None,
            web_url: None,
            qsl_manager: None,
            eqsl: 0,
            lotw: 0,
            paper_qsl: 0,
            cq_zone: None,
            itu_zone: None,
            iota: None,
            dxcc_country_name: None,
            dxcc_continent: None,
            birth_year: None,
            qrz_serial: None,
            last_modified: None,
            bio_length: None,
            image_url: None,
            msa: None,
            area_code: None,
            time_zone: None,
            gmt_offset: None,
            dst_observed: None,
            profile_views: None,
        }),
        error_message: Some(
            "Lookup transport is live, but provider-backed callsign lookup is not implemented yet."
                .into(),
        ),
        cache_hit: false,
        lookup_latency_ms: 0,
        queried_callsign: normalized_callsign,
    }
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::{normalize_callsign, placeholder_lookup_error};
    use crate::proto::logripper::domain::LookupState;

    #[test]
    fn normalize_callsign_trims_uppercases_and_defaults_blank_input() {
        assert_eq!(normalize_callsign("  w1aw  "), "W1AW");
        assert_eq!(normalize_callsign(" \t "), "K7DBG");
    }

    #[test]
    fn placeholder_lookup_error_uses_normalized_callsign_in_result_and_record() {
        let result = placeholder_lookup_error("  w1aw ");

        assert_eq!(result.state, LookupState::Error as i32);
        assert_eq!(result.queried_callsign, "W1AW");
        assert_eq!(
            result.error_message.as_deref(),
            Some("Lookup transport is live, but provider-backed callsign lookup is not implemented yet.")
        );

        let Some(record) = result.record else {
            panic!("Expected placeholder lookup result to include a callsign record");
        };

        assert_eq!(record.callsign, "W1AW");
        assert_eq!(record.cross_ref, "W1AW");
        assert_eq!(
            record.formatted_name.as_deref(),
            Some("Developer Placeholder")
        );
    }
}
