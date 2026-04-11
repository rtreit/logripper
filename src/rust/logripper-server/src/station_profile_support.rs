use std::collections::BTreeMap;

use logripper_core::domain::station::station_profile_has_values;
use logripper_core::proto::logripper::domain::StationProfile;

use crate::runtime_config::{
    STATION_CALLSIGN_ENV_VAR, STATION_COUNTRY_ENV_VAR, STATION_COUNTY_ENV_VAR,
    STATION_CQ_ZONE_ENV_VAR, STATION_DXCC_ENV_VAR, STATION_GRID_ENV_VAR, STATION_ITU_ZONE_ENV_VAR,
    STATION_LATITUDE_ENV_VAR, STATION_LONGITUDE_ENV_VAR, STATION_OPERATOR_CALLSIGN_ENV_VAR,
    STATION_OPERATOR_NAME_ENV_VAR, STATION_PROFILE_NAME_ENV_VAR, STATION_STATE_ENV_VAR,
};

pub(crate) const DEFAULT_PROFILE_NAME: &str = "Home";

pub(crate) fn normalize_station_profile(
    mut profile: StationProfile,
    normalize_callsign: impl Fn(Option<&str>) -> Option<String>,
    normalize_string: impl Fn(Option<&str>) -> Option<String>,
) -> Result<StationProfile, String> {
    let station_callsign = normalize_callsign(Some(profile.station_callsign.as_str()))
        .ok_or_else(|| "station_profile.station_callsign is required.".to_string())?;
    profile.profile_name = normalize_string(profile.profile_name.as_deref())
        .or_else(|| Some(DEFAULT_PROFILE_NAME.to_string()));
    profile.station_callsign.clone_from(&station_callsign);
    profile.operator_callsign =
        normalize_callsign(profile.operator_callsign.as_deref()).or(Some(station_callsign));
    profile.operator_name = normalize_string(profile.operator_name.as_deref());
    profile.grid = normalize_string(profile.grid.as_deref());
    profile.county = normalize_string(profile.county.as_deref());
    profile.state = normalize_string(profile.state.as_deref());
    profile.country = normalize_string(profile.country.as_deref());
    profile.dxcc = validate_optional_positive_integer("station_profile.dxcc", profile.dxcc)?;
    profile.cq_zone =
        validate_optional_positive_integer("station_profile.cq_zone", profile.cq_zone)?;
    profile.itu_zone =
        validate_optional_positive_integer("station_profile.itu_zone", profile.itu_zone)?;
    profile.latitude =
        validate_optional_bounded_float("station_profile.latitude", profile.latitude, -90.0, 90.0)?;
    profile.longitude = validate_optional_bounded_float(
        "station_profile.longitude",
        profile.longitude,
        -180.0,
        180.0,
    )?;

    if !station_profile_has_values(&profile) {
        return Err("station_profile must include at least a station_callsign.".to_string());
    }

    Ok(profile)
}

pub(crate) fn insert_station_profile_runtime_values(
    values: &mut BTreeMap<String, String>,
    profile: &StationProfile,
) {
    insert_optional_string(
        values,
        STATION_PROFILE_NAME_ENV_VAR,
        profile.profile_name.as_deref(),
    );
    insert_optional_string(
        values,
        STATION_CALLSIGN_ENV_VAR,
        Some(profile.station_callsign.as_str()),
    );
    insert_optional_string(
        values,
        STATION_OPERATOR_CALLSIGN_ENV_VAR,
        profile.operator_callsign.as_deref(),
    );
    insert_optional_string(
        values,
        STATION_OPERATOR_NAME_ENV_VAR,
        profile.operator_name.as_deref(),
    );
    insert_optional_string(values, STATION_GRID_ENV_VAR, profile.grid.as_deref());
    insert_optional_string(values, STATION_COUNTY_ENV_VAR, profile.county.as_deref());
    insert_optional_string(values, STATION_STATE_ENV_VAR, profile.state.as_deref());
    insert_optional_string(values, STATION_COUNTRY_ENV_VAR, profile.country.as_deref());
    insert_optional_number(values, STATION_DXCC_ENV_VAR, profile.dxcc);
    insert_optional_number(values, STATION_CQ_ZONE_ENV_VAR, profile.cq_zone);
    insert_optional_number(values, STATION_ITU_ZONE_ENV_VAR, profile.itu_zone);
    insert_optional_float(values, STATION_LATITUDE_ENV_VAR, profile.latitude);
    insert_optional_float(values, STATION_LONGITUDE_ENV_VAR, profile.longitude);
}

pub(crate) fn validate_optional_positive_integer(
    label: &str,
    value: Option<u32>,
) -> Result<Option<u32>, String> {
    match value {
        Some(0) => Err(format!("{label} must be greater than 0.")),
        Some(value) => Ok(Some(value)),
        None => Ok(None),
    }
}

pub(crate) fn validate_optional_bounded_float(
    label: &str,
    value: Option<f64>,
    min: f64,
    max: f64,
) -> Result<Option<f64>, String> {
    let Some(value) = value else {
        return Ok(None);
    };

    if !value.is_finite() {
        return Err(format!("{label} must be a finite decimal value."));
    }
    if value < min || value > max {
        return Err(format!("{label} must be between {min} and {max}."));
    }

    Ok(Some(value))
}

fn insert_optional_string(values: &mut BTreeMap<String, String>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        values.insert(key.to_string(), value.to_string());
    }
}

fn insert_optional_number<T>(values: &mut BTreeMap<String, String>, key: &str, value: Option<T>)
where
    T: ToString,
{
    if let Some(value) = value {
        values.insert(key.to_string(), value.to_string());
    }
}

fn insert_optional_float(values: &mut BTreeMap<String, String>, key: &str, value: Option<f64>) {
    if let Some(value) = value {
        values.insert(key.to_string(), value.to_string());
    }
}
