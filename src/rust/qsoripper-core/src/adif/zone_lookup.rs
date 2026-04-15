//! CQ zone resolution from location data (state/province and coordinates).
//!
//! This module provides a middle tier in the zone-enrichment cascade:
//!
//! 1. **QRZ-provided zone** – kept as-is when present.
//! 2. **Location-derived zone** – this module: uses state/province + DXCC
//!    entity mapping, or lat/lon coordinates as fallback.
//! 3. **DXCC entity default** – single default per entity (existing fallback).

use crate::proto::qsoripper::domain::CallsignRecord;

/// Derive missing CQ zone from location data (state/province or coordinates).
///
/// This runs **before** the DXCC entity default fallback, providing more
/// accurate zones for multi-zone countries like the USA, Canada, and
/// Australia.
pub(crate) fn enrich_zones_from_location(record: &mut CallsignRecord) {
    if record.cq_zone.is_some() {
        return;
    }

    // Step 1: Try state/subdivision mapping (most accurate for supported
    // entities).
    if let Some(zone) = cq_zone_from_subdivision(record.dxcc_entity_id, record.state.as_deref()) {
        record.cq_zone = Some(zone);
        return;
    }

    // Step 2: Try coordinate-based derivation.
    let (lat, lon) = match (record.latitude, record.longitude) {
        (Some(lat), Some(lon)) => (lat, lon),
        _ => {
            // Try grid square → lat/lon fallback.
            match record
                .grid_square
                .as_deref()
                .and_then(super::maidenhead::grid_to_latlon)
            {
                Some((lat, lon)) => (lat, lon),
                None => return,
            }
        }
    };

    if let Some(zone) = cq_zone_from_coordinates(record.dxcc_entity_id, lat, lon) {
        record.cq_zone = Some(zone);
    }
}

/// Map a US state, Canadian province, or Australian state/territory to its
/// CQ zone using the DXCC entity id and subdivision code.
fn cq_zone_from_subdivision(dxcc: u32, state: Option<&str>) -> Option<u32> {
    let state = state?.trim();
    if state.is_empty() {
        return None;
    }
    let upper: String = state.to_ascii_uppercase();
    let upper = upper.as_str();

    match dxcc {
        // USA (lower 48 + DC) – DXCC 291
        291 => match upper {
            // Zone 3 – Western
            "AZ" | "CA" | "ID" | "MT" | "NV" | "NM" | "OR" | "UT" | "WA" | "WY" => Some(3),
            // Zone 4 – Central
            "CO" | "AR" | "IA" | "KS" | "LA" | "MN" | "MO" | "MS" | "NE" | "ND" | "OK" | "SD"
            | "TX" | "WI" => Some(4),
            // Zone 5 – Eastern
            "AL" | "CT" | "DC" | "DE" | "FL" | "GA" | "IL" | "IN" | "KY" | "MA" | "MD" | "ME"
            | "MI" | "NC" | "NH" | "NJ" | "NY" | "OH" | "PA" | "RI" | "SC" | "TN" | "VA" | "VT"
            | "WV" => Some(5),
            _ => None,
        },
        // Canada – DXCC 1
        1 => match upper {
            // Zone 1 – Northern territories
            "YT" | "NT" | "NU" => Some(1),
            // Zone 2 – Newfoundland & Labrador
            "NL" => Some(2),
            // Zone 3 – British Columbia
            "BC" => Some(3),
            // Zone 4 – Southern provinces
            "AB" | "SK" | "MB" | "ON" | "QC" | "NB" | "NS" | "PE" => Some(4),
            _ => None,
        },
        // Australia – DXCC 150
        150 => match upper {
            // Zone 29 – Western Australia
            "WA" => Some(29),
            // Zone 30 – Rest of Australia
            "NT" | "SA" | "QLD" | "NSW" | "VIC" | "TAS" | "ACT" => Some(30),
            _ => None,
        },
        _ => None,
    }
}

/// Approximate CQ zone from latitude/longitude when state data is
/// unavailable.
fn cq_zone_from_coordinates(dxcc: u32, lat: f64, lon: f64) -> Option<u32> {
    match dxcc {
        // USA (lower 48 + DC) – longitude boundaries
        291 => {
            if lon <= -105.0 {
                Some(3)
            } else if lon <= -90.0 {
                Some(4)
            } else {
                Some(5)
            }
        }
        // Canada – latitude + longitude boundaries
        1 => {
            if lat > 60.0 && lon < -110.0 {
                Some(1)
            } else if lat > 53.0 && lon > -66.0 {
                Some(2)
            } else if lon < -110.0 {
                Some(3)
            } else {
                Some(4)
            }
        }
        // Australia – longitude boundary
        150 => {
            if lon < 130.0 {
                Some(29)
            } else {
                Some(30)
            }
        }
        _ => None,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn us_record_with_state(state: &str) -> CallsignRecord {
        CallsignRecord {
            dxcc_entity_id: 291,
            state: Some(state.to_owned()),
            ..CallsignRecord::default()
        }
    }

    // ── US state mapping ────────────────────────────────────────────────

    #[test]
    fn us_wa_maps_to_zone_3() {
        let mut r = us_record_with_state("WA");
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(3));
    }

    #[test]
    fn us_ny_maps_to_zone_5() {
        let mut r = us_record_with_state("NY");
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(5));
    }

    #[test]
    fn us_tx_maps_to_zone_4() {
        let mut r = us_record_with_state("TX");
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(4));
    }

    #[test]
    fn us_co_maps_to_zone_4() {
        let mut r = us_record_with_state("CO");
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(4));
    }

    // ── Canadian province mapping ───────────────────────────────────────

    #[test]
    fn ca_bc_maps_to_zone_3() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 1,
            state: Some("BC".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(3));
    }

    #[test]
    fn ca_on_maps_to_zone_4() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 1,
            state: Some("ON".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(4));
    }

    #[test]
    fn ca_nl_maps_to_zone_2() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 1,
            state: Some("NL".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(2));
    }

    #[test]
    fn ca_yt_maps_to_zone_1() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 1,
            state: Some("YT".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(1));
    }

    // ── Australian state mapping ────────────────────────────────────────

    #[test]
    fn au_wa_maps_to_zone_29() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 150,
            state: Some("WA".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(29));
    }

    #[test]
    fn au_nsw_maps_to_zone_30() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 150,
            state: Some("NSW".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(30));
    }

    // ── Coordinate fallback ─────────────────────────────────────────────

    #[test]
    fn us_coords_western_maps_to_zone_3() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 291,
            latitude: Some(47.5),
            longitude: Some(-122.5),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(3));
    }

    #[test]
    fn us_coords_central_maps_to_zone_4() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 291,
            latitude: Some(32.0),
            longitude: Some(-97.0),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(4));
    }

    #[test]
    fn us_coords_eastern_maps_to_zone_5() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 291,
            latitude: Some(40.7),
            longitude: Some(-74.0),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(5));
    }

    // ── Grid-square fallback ────────────────────────────────────────────

    #[test]
    fn us_grid_cn87_maps_to_zone_3() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 291,
            grid_square: Some("CN87".to_owned()),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(3));
    }

    // ── Unknown entity falls through ────────────────────────────────────

    #[test]
    fn unknown_entity_returns_none() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 9999,
            state: Some("XX".to_owned()),
            latitude: Some(0.0),
            longitude: Some(0.0),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, None);
    }

    // ── Existing zone preserved ─────────────────────────────────────────

    #[test]
    fn existing_zone_is_preserved() {
        let mut r = CallsignRecord {
            dxcc_entity_id: 291,
            state: Some("WA".to_owned()),
            cq_zone: Some(99),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(99));
    }

    // ── State takes priority over coordinates ───────────────────────────

    #[test]
    fn state_takes_priority_over_coordinates() {
        // WA state → zone 3, but eastern longitude would give zone 5
        let mut r = CallsignRecord {
            dxcc_entity_id: 291,
            state: Some("WA".to_owned()),
            latitude: Some(40.0),
            longitude: Some(-74.0),
            ..CallsignRecord::default()
        };
        enrich_zones_from_location(&mut r);
        assert_eq!(r.cq_zone, Some(3));
    }
}
