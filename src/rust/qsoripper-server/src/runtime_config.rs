use std::collections::BTreeMap;
use std::sync::Arc;

use qsoripper_core::application::logbook::LogbookEngine;
use qsoripper_core::domain::lookup::normalize_callsign;
use qsoripper_core::domain::station::station_profile_has_values;
use qsoripper_core::lookup::{
    CallsignProvider, DisabledCallsignProvider, LookupCoordinator, LookupCoordinatorConfig,
    QrzXmlConfig, QrzXmlProvider, DEFAULT_QRZ_XML_BASE_URL, QRZ_HTTP_TIMEOUT_SECONDS_ENV_VAR,
    QRZ_MAX_RETRIES_ENV_VAR, QRZ_USER_AGENT_ENV_VAR, QRZ_XML_BASE_URL_ENV_VAR,
    QRZ_XML_CAPTURE_ONLY_ENV_VAR, QRZ_XML_PASSWORD_ENV_VAR, QRZ_XML_USERNAME_ENV_VAR,
};
use qsoripper_core::proto::qsoripper::domain::StationProfile;
use qsoripper_core::proto::qsoripper::services::{
    ApplyRuntimeConfigRequest, ResetRuntimeConfigRequest, RuntimeConfigDefinition,
    RuntimeConfigMutation, RuntimeConfigMutationKind, RuntimeConfigSnapshot, RuntimeConfigValue,
    RuntimeConfigValueKind,
};
use tokio::sync::RwLock;

use crate::station_profile_support::{
    insert_station_profile_runtime_values, normalize_station_profile,
};
use crate::{build_storage, parse_storage_backend, StorageOptions};

pub(crate) const STORAGE_BACKEND_ENV_VAR: &str = "QSORIPPER_STORAGE_BACKEND";
pub(crate) const SQLITE_PATH_ENV_VAR: &str = "QSORIPPER_SQLITE_PATH";
pub(crate) const STATION_PROFILE_NAME_ENV_VAR: &str = "QSORIPPER_STATION_PROFILE_NAME";
pub(crate) const STATION_CALLSIGN_ENV_VAR: &str = "QSORIPPER_STATION_CALLSIGN";
pub(crate) const STATION_OPERATOR_CALLSIGN_ENV_VAR: &str = "QSORIPPER_STATION_OPERATOR_CALLSIGN";
pub(crate) const STATION_OPERATOR_NAME_ENV_VAR: &str = "QSORIPPER_STATION_OPERATOR_NAME";
pub(crate) const STATION_GRID_ENV_VAR: &str = "QSORIPPER_STATION_GRID";
pub(crate) const STATION_COUNTY_ENV_VAR: &str = "QSORIPPER_STATION_COUNTY";
pub(crate) const STATION_STATE_ENV_VAR: &str = "QSORIPPER_STATION_STATE";
pub(crate) const STATION_COUNTRY_ENV_VAR: &str = "QSORIPPER_STATION_COUNTRY";
pub(crate) const STATION_ARRL_SECTION_ENV_VAR: &str = "QSORIPPER_STATION_ARRL_SECTION";
pub(crate) const STATION_DXCC_ENV_VAR: &str = "QSORIPPER_STATION_DXCC";
pub(crate) const STATION_CQ_ZONE_ENV_VAR: &str = "QSORIPPER_STATION_CQ_ZONE";
pub(crate) const STATION_ITU_ZONE_ENV_VAR: &str = "QSORIPPER_STATION_ITU_ZONE";
pub(crate) const STATION_LATITUDE_ENV_VAR: &str = "QSORIPPER_STATION_LATITUDE";
pub(crate) const STATION_LONGITUDE_ENV_VAR: &str = "QSORIPPER_STATION_LONGITUDE";
const DEFAULT_STORAGE_BACKEND: &str = "memory";
const DEFAULT_SQLITE_PATH: &str = "qsoripper.db";
const REDACTED_VALUE: &str = "<redacted>";

#[derive(Clone)]
struct RuntimeBindings {
    logbook_engine: LogbookEngine,
    lookup_coordinator: Arc<LookupCoordinator>,
    active_storage_backend: String,
    lookup_provider_summary: String,
    active_station_profile: Option<StationProfile>,
}

pub(crate) struct RuntimeConfigManager {
    config_file_values: RwLock<BTreeMap<String, String>>,
    startup_values: BTreeMap<String, String>,
    session_station_profile_override: RwLock<Option<StationProfile>>,
    overrides: RwLock<BTreeMap<String, String>>,
    bindings: RwLock<RuntimeBindings>,
}

impl RuntimeConfigManager {
    pub(crate) fn new_with_config_file_values_and_cli_storage_overrides(
        config_file_values: BTreeMap<String, String>,
        cli_storage_overrides: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let startup_values = capture_supported_env();
        Self::new_with_config_file_values(
            merge_values(&startup_values, cli_storage_overrides),
            config_file_values,
        )
    }

    #[cfg(test)]
    pub(crate) fn new(startup_values: BTreeMap<String, String>) -> Result<Self, String> {
        Self::new_with_config_file_values(startup_values, BTreeMap::new())
    }

    pub(crate) fn new_with_config_file_values(
        startup_values: BTreeMap<String, String>,
        config_file_values: BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let effective_values = merged_base_values(&config_file_values, &startup_values);
        let bindings = build_runtime_bindings(&effective_values)?;
        Ok(Self {
            config_file_values: RwLock::new(config_file_values),
            startup_values,
            session_station_profile_override: RwLock::new(None),
            overrides: RwLock::new(BTreeMap::new()),
            bindings: RwLock::new(bindings),
        })
    }

    pub(crate) async fn snapshot(&self) -> RuntimeConfigSnapshot {
        let config_file_values = self.config_file_values.read().await.clone();
        let session_station_profile_override =
            self.session_station_profile_override.read().await.clone();
        let overrides = self.overrides.read().await.clone();
        let bindings = self.bindings.read().await.clone();
        let base_values = merged_base_values(&config_file_values, &self.startup_values);
        build_snapshot(
            &base_values,
            session_station_profile_override.as_ref(),
            &overrides,
            &bindings,
        )
    }

    pub(crate) async fn apply_request(
        &self,
        request: ApplyRuntimeConfigRequest,
    ) -> Result<RuntimeConfigSnapshot, String> {
        let mut next_overrides = self.overrides.read().await.clone();

        for mutation in request.mutations {
            apply_mutation(&mut next_overrides, mutation)?;
        }

        self.swap_runtime(next_overrides).await
    }

    pub(crate) async fn reset_request(
        &self,
        request: ResetRuntimeConfigRequest,
    ) -> Result<RuntimeConfigSnapshot, String> {
        let mut next_overrides = self.overrides.read().await.clone();

        if request.keys.is_empty() {
            next_overrides.clear();
        } else {
            for raw_key in request.keys {
                let key = canonical_key(&raw_key)?;
                next_overrides.remove(key);
            }
        }

        self.swap_runtime(next_overrides).await
    }

    pub(crate) async fn logbook_engine(&self) -> LogbookEngine {
        self.bindings.read().await.logbook_engine.clone()
    }

    pub(crate) async fn logbook_context(&self) -> (LogbookEngine, Option<StationProfile>) {
        let bindings = self.bindings.read().await;
        (
            bindings.logbook_engine.clone(),
            bindings.active_station_profile.clone(),
        )
    }

    pub(crate) async fn lookup_coordinator(&self) -> Arc<LookupCoordinator> {
        self.bindings.read().await.lookup_coordinator.clone()
    }

    pub(crate) async fn active_storage_backend(&self) -> String {
        self.bindings.read().await.active_storage_backend.clone()
    }

    pub(crate) async fn effective_values(&self) -> BTreeMap<String, String> {
        let config_file_values = self.config_file_values.read().await.clone();
        let overrides = self.overrides.read().await.clone();
        let base = merged_base_values(&config_file_values, &self.startup_values);
        merge_values(&base, &overrides)
    }

    pub(crate) async fn preview_config_file_values(
        &self,
        next_config_file_values: BTreeMap<String, String>,
    ) -> Result<(), String> {
        let session_station_profile_override =
            self.session_station_profile_override.read().await.clone();
        let overrides = self.overrides.read().await.clone();
        let base_values = merged_base_values(&next_config_file_values, &self.startup_values);
        let effective_values = build_effective_values(
            &base_values,
            session_station_profile_override.as_ref(),
            &overrides,
        );
        build_runtime_bindings(&effective_values).map(|_| ())
    }

    pub(crate) async fn replace_config_file_values(
        &self,
        next_config_file_values: BTreeMap<String, String>,
    ) -> Result<RuntimeConfigSnapshot, String> {
        let session_station_profile_override =
            self.session_station_profile_override.read().await.clone();
        let overrides = self.overrides.read().await.clone();
        let base_values = merged_base_values(&next_config_file_values, &self.startup_values);
        let effective_values = build_effective_values(
            &base_values,
            session_station_profile_override.as_ref(),
            &overrides,
        );
        let next_bindings = build_runtime_bindings(&effective_values)?;

        {
            let mut bindings = self.bindings.write().await;
            *bindings = next_bindings;
        }

        {
            let mut config_file_values = self.config_file_values.write().await;
            *config_file_values = next_config_file_values;
        }

        Ok(self.snapshot().await)
    }

    pub(crate) async fn set_session_station_profile_override(
        &self,
        profile: Option<StationProfile>,
    ) -> Result<Option<StationProfile>, String> {
        let normalized = profile
            .map(|profile| {
                normalize_station_profile(
                    profile,
                    normalize_optional_station_callsign,
                    normalize_optional_runtime_string,
                )
            })
            .transpose()?;
        let config_file_values = self.config_file_values.read().await.clone();
        let overrides = self.overrides.read().await.clone();
        let base_values = merged_base_values(&config_file_values, &self.startup_values);
        let effective_values =
            build_effective_values(&base_values, normalized.as_ref(), &overrides);
        let next_bindings = build_runtime_bindings(&effective_values)?;

        {
            let mut bindings = self.bindings.write().await;
            *bindings = next_bindings;
        }

        {
            let mut session_override = self.session_station_profile_override.write().await;
            session_override.clone_from(&normalized);
        }

        Ok(normalized)
    }

    pub(crate) async fn session_station_profile_override(&self) -> Option<StationProfile> {
        self.session_station_profile_override.read().await.clone()
    }

    pub(crate) async fn effective_station_profile(&self) -> Option<StationProfile> {
        self.bindings.read().await.active_station_profile.clone()
    }

    async fn swap_runtime(
        &self,
        next_overrides: BTreeMap<String, String>,
    ) -> Result<RuntimeConfigSnapshot, String> {
        let config_file_values = self.config_file_values.read().await.clone();
        let session_station_profile_override =
            self.session_station_profile_override.read().await.clone();
        let base_values = merged_base_values(&config_file_values, &self.startup_values);
        let effective_values = build_effective_values(
            &base_values,
            session_station_profile_override.as_ref(),
            &next_overrides,
        );
        let next_bindings = build_runtime_bindings(&effective_values)?;

        {
            let mut bindings = self.bindings.write().await;
            *bindings = next_bindings;
        }

        {
            let mut overrides = self.overrides.write().await;
            *overrides = next_overrides;
        }

        Ok(self.snapshot().await)
    }
}

struct ConfigFieldSpec {
    key: &'static str,
    label: &'static str,
    description: &'static str,
    kind: RuntimeConfigValueKind,
    secret: bool,
    allowed_values: &'static [&'static str],
    default_value: Option<&'static str>,
}

const STORAGE_ALLOWED_VALUES: &[&str] = &["memory", "sqlite"];
const BOOLEAN_ALLOWED_VALUES: &[&str] = &["true", "false"];

const SUPPORTED_FIELDS: &[ConfigFieldSpec] = &[
    ConfigFieldSpec {
        key: STORAGE_BACKEND_ENV_VAR,
        label: "Storage backend",
        description: "Hot-swap the active logbook storage implementation for new requests.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: STORAGE_ALLOWED_VALUES,
        default_value: Some(DEFAULT_STORAGE_BACKEND),
    },
    ConfigFieldSpec {
        key: SQLITE_PATH_ENV_VAR,
        label: "SQLite path",
        description: "SQLite database path used when the active storage backend is sqlite.",
        kind: RuntimeConfigValueKind::Path,
        secret: false,
        allowed_values: &[],
        default_value: Some(DEFAULT_SQLITE_PATH),
    },
    ConfigFieldSpec {
        key: STATION_PROFILE_NAME_ENV_VAR,
        label: "Station profile name",
        description: "Friendly label shown for the active local-station profile.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_CALLSIGN_ENV_VAR,
        label: "Station callsign",
        description: "Default local station callsign used when logging new QSOs.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_OPERATOR_CALLSIGN_ENV_VAR,
        label: "Operator callsign",
        description: "Operator callsign captured in the saved station snapshot.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_OPERATOR_NAME_ENV_VAR,
        label: "Operator name",
        description: "Human-readable operator name captured in the saved station snapshot.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_GRID_ENV_VAR,
        label: "Station grid",
        description: "Default local station Maidenhead grid square.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_COUNTY_ENV_VAR,
        label: "Station county",
        description: "Default local station county for saved QSOs.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_STATE_ENV_VAR,
        label: "Station state",
        description: "Default local station state or province.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_COUNTRY_ENV_VAR,
        label: "Station country",
        description: "Default local station country name.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_ARRL_SECTION_ENV_VAR,
        label: "Station ARRL section",
        description: "Default local station ARRL section.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_DXCC_ENV_VAR,
        label: "Station DXCC",
        description: "Default local station DXCC entity code.",
        kind: RuntimeConfigValueKind::Integer,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_CQ_ZONE_ENV_VAR,
        label: "Station CQ zone",
        description: "Default local station CQ zone.",
        kind: RuntimeConfigValueKind::Integer,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_ITU_ZONE_ENV_VAR,
        label: "Station ITU zone",
        description: "Default local station ITU zone.",
        kind: RuntimeConfigValueKind::Integer,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_LATITUDE_ENV_VAR,
        label: "Station latitude",
        description: "Default local station latitude in decimal degrees.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: STATION_LONGITUDE_ENV_VAR,
        label: "Station longitude",
        description: "Default local station longitude in decimal degrees.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: QRZ_XML_USERNAME_ENV_VAR,
        label: "QRZ XML username",
        description: "QRZ XML login username for live callsign lookups.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: QRZ_XML_PASSWORD_ENV_VAR,
        label: "QRZ XML password",
        description: "QRZ XML login password. The live snapshot always redacts this value.",
        kind: RuntimeConfigValueKind::String,
        secret: true,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: QRZ_USER_AGENT_ENV_VAR,
        label: "QRZ user agent",
        description: "User agent string supplied to QRZ XML requests.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: None,
    },
    ConfigFieldSpec {
        key: QRZ_XML_BASE_URL_ENV_VAR,
        label: "QRZ XML base URL",
        description: "QRZ XML endpoint used by the live lookup provider.",
        kind: RuntimeConfigValueKind::String,
        secret: false,
        allowed_values: &[],
        default_value: Some(DEFAULT_QRZ_XML_BASE_URL),
    },
    ConfigFieldSpec {
        key: QRZ_HTTP_TIMEOUT_SECONDS_ENV_VAR,
        label: "QRZ HTTP timeout seconds",
        description: "HTTP timeout used by live QRZ XML requests.",
        kind: RuntimeConfigValueKind::Integer,
        secret: false,
        allowed_values: &[],
        default_value: Some("8"),
    },
    ConfigFieldSpec {
        key: QRZ_MAX_RETRIES_ENV_VAR,
        label: "QRZ max retries",
        description: "Retry count for retryable QRZ XML transport failures.",
        kind: RuntimeConfigValueKind::Integer,
        secret: false,
        allowed_values: &[],
        default_value: Some("2"),
    },
    ConfigFieldSpec {
        key: QRZ_XML_CAPTURE_ONLY_ENV_VAR,
        label: "QRZ capture-only mode",
        description:
            "When true, capture and redact the outbound QRZ request instead of sending it.",
        kind: RuntimeConfigValueKind::Boolean,
        secret: false,
        allowed_values: BOOLEAN_ALLOWED_VALUES,
        default_value: Some("false"),
    },
];

fn capture_supported_env() -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();

    for field in SUPPORTED_FIELDS {
        if let Ok(value) = std::env::var(field.key) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                values.insert(field.key.to_string(), trimmed.to_string());
            }
        }
    }

    values
}

fn apply_mutation(
    overrides: &mut BTreeMap<String, String>,
    mutation: RuntimeConfigMutation,
) -> Result<(), String> {
    let key = canonical_key(&mutation.key)?;
    let action = RuntimeConfigMutationKind::try_from(mutation.kind)
        .map_err(|_| format!("Unsupported mutation kind for '{key}'."))?;

    match action {
        RuntimeConfigMutationKind::Set => {
            let value = mutation
                .value
                .ok_or_else(|| format!("A value is required when setting '{key}'."))?;
            let normalized = normalize_value(key, &value)?;
            overrides.insert(key.to_string(), normalized);
        }
        RuntimeConfigMutationKind::Clear => {
            overrides.remove(key);
        }
        RuntimeConfigMutationKind::Unspecified => {
            return Err(format!("A mutation kind is required for '{key}'."));
        }
    }

    Ok(())
}

fn canonical_key(raw_key: &str) -> Result<&'static str, String> {
    let trimmed = raw_key.trim();
    SUPPORTED_FIELDS
        .iter()
        .find(|field| field.key.eq_ignore_ascii_case(trimmed))
        .map(|field| field.key)
        .ok_or_else(|| format!("Unsupported runtime config key '{trimmed}'."))
}

fn normalize_value(key: &'static str, raw_value: &str) -> Result<String, String> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return Err(format!("A non-empty value is required for '{key}'."));
    }

    if key == STORAGE_BACKEND_ENV_VAR {
        parse_storage_backend(trimmed)
            .map_err(|error| error.to_string())
            .map(|_| trimmed.to_ascii_lowercase())
    } else if key == QRZ_XML_CAPTURE_ONLY_ENV_VAR {
        parse_bool(trimmed).map(|value| {
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        })
    } else if key == QRZ_HTTP_TIMEOUT_SECONDS_ENV_VAR {
        trimmed
            .parse::<u64>()
            .map(|value| value.to_string())
            .map_err(|_| format!("'{key}' expects an integer value."))
    } else if key == QRZ_MAX_RETRIES_ENV_VAR {
        trimmed
            .parse::<u32>()
            .map(|value| value.to_string())
            .map_err(|_| format!("'{key}' expects an integer value."))
    } else if is_station_positive_integer_key(key) {
        parse_positive_integer(key, trimmed).map(|value| value.to_string())
    } else if key == STATION_LATITUDE_ENV_VAR {
        parse_bounded_f64(key, trimmed, -90.0, 90.0).map(|value| value.to_string())
    } else if key == STATION_LONGITUDE_ENV_VAR {
        parse_bounded_f64(key, trimmed, -180.0, 180.0).map(|value| value.to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

fn parse_bool(raw_value: &str) -> Result<bool, String> {
    match raw_value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Ok(true),
        "0" | "false" | "no" | "n" | "off" => Ok(false),
        _ => Err(format!(
            "'{raw_value}' is not a valid boolean. Use true/false, yes/no, 1/0, y/n, or on/off."
        )),
    }
}

fn parse_positive_integer(key: &str, raw_value: &str) -> Result<u32, String> {
    let value = raw_value
        .parse::<u32>()
        .map_err(|_| format!("'{key}' expects an integer value."))?;
    if value == 0 {
        return Err(format!("'{key}' expects an integer greater than 0."));
    }
    Ok(value)
}

fn parse_bounded_f64(key: &str, raw_value: &str, min: f64, max: f64) -> Result<f64, String> {
    let value = raw_value
        .parse::<f64>()
        .map_err(|_| format!("'{key}' expects a decimal value."))?;
    if !value.is_finite() {
        return Err(format!("'{key}' expects a finite decimal value."));
    }
    if value < min || value > max {
        return Err(format!("'{key}' must be between {min} and {max}."));
    }
    Ok(value)
}

fn is_station_positive_integer_key(key: &str) -> bool {
    matches!(
        key,
        STATION_DXCC_ENV_VAR | STATION_CQ_ZONE_ENV_VAR | STATION_ITU_ZONE_ENV_VAR
    )
}

fn merge_values(
    base_values: &BTreeMap<String, String>,
    overrides: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = base_values.clone();
    merged.extend(overrides.clone());
    merged
}

fn merged_base_values(
    config_file_values: &BTreeMap<String, String>,
    startup_values: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    merge_values(config_file_values, startup_values)
}

fn build_effective_values(
    base_values: &BTreeMap<String, String>,
    session_station_profile_override: Option<&StationProfile>,
    overrides: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let session_values = station_profile_override_values(session_station_profile_override);
    let with_session_override = merge_values(base_values, &session_values);
    merge_values(&with_session_override, overrides)
}

fn station_profile_override_values(profile: Option<&StationProfile>) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    if let Some(profile) = profile {
        insert_station_profile_runtime_values(&mut values, profile);
    }
    values
}

fn build_runtime_bindings(values: &BTreeMap<String, String>) -> Result<RuntimeBindings, String> {
    let storage = build_storage(
        &parse_storage_options_from_values(values).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let logbook_engine = LogbookEngine::new(storage);
    let active_storage_backend = logbook_engine.storage_backend_name().to_string();
    let (provider, lookup_provider_summary) = build_lookup_provider(values);
    let lookup_coordinator = Arc::new(LookupCoordinator::new(
        provider,
        LookupCoordinatorConfig::default(),
    ));
    let active_station_profile = build_active_station_profile(values)?;

    Ok(RuntimeBindings {
        logbook_engine,
        lookup_coordinator,
        active_storage_backend,
        lookup_provider_summary,
        active_station_profile,
    })
}

fn build_active_station_profile(
    values: &BTreeMap<String, String>,
) -> Result<Option<StationProfile>, String> {
    let profile = StationProfile {
        profile_name: values.get(STATION_PROFILE_NAME_ENV_VAR).cloned(),
        station_callsign: values
            .get(STATION_CALLSIGN_ENV_VAR)
            .cloned()
            .unwrap_or_default(),
        operator_callsign: values.get(STATION_OPERATOR_CALLSIGN_ENV_VAR).cloned(),
        operator_name: values.get(STATION_OPERATOR_NAME_ENV_VAR).cloned(),
        grid: values.get(STATION_GRID_ENV_VAR).cloned(),
        county: values.get(STATION_COUNTY_ENV_VAR).cloned(),
        state: values.get(STATION_STATE_ENV_VAR).cloned(),
        country: values.get(STATION_COUNTRY_ENV_VAR).cloned(),
        arrl_section: values.get(STATION_ARRL_SECTION_ENV_VAR).cloned(),
        dxcc: parse_optional_positive_integer(values, STATION_DXCC_ENV_VAR)?,
        cq_zone: parse_optional_positive_integer(values, STATION_CQ_ZONE_ENV_VAR)?,
        itu_zone: parse_optional_positive_integer(values, STATION_ITU_ZONE_ENV_VAR)?,
        latitude: parse_optional_bounded_f64(values, STATION_LATITUDE_ENV_VAR, -90.0, 90.0)?,
        longitude: parse_optional_bounded_f64(values, STATION_LONGITUDE_ENV_VAR, -180.0, 180.0)?,
    };

    Ok(station_profile_has_values(&profile).then_some(profile))
}

fn parse_optional_positive_integer(
    values: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<u32>, String> {
    values
        .get(key)
        .map(|value| parse_positive_integer(key, value))
        .transpose()
}

fn parse_optional_bounded_f64(
    values: &BTreeMap<String, String>,
    key: &str,
    min: f64,
    max: f64,
) -> Result<Option<f64>, String> {
    values
        .get(key)
        .map(|value| parse_bounded_f64(key, value, min, max))
        .transpose()
}

fn parse_storage_options_from_values(
    values: &BTreeMap<String, String>,
) -> Result<StorageOptions, Box<dyn std::error::Error>> {
    let backend = parse_storage_backend(
        values
            .get(STORAGE_BACKEND_ENV_VAR)
            .map_or(DEFAULT_STORAGE_BACKEND, String::as_str),
    )?;
    let sqlite_path = values
        .get(SQLITE_PATH_ENV_VAR)
        .cloned()
        .unwrap_or_else(|| DEFAULT_SQLITE_PATH.to_string())
        .into();

    Ok(StorageOptions {
        backend,
        sqlite_path,
    })
}

fn build_lookup_provider(values: &BTreeMap<String, String>) -> (Arc<dyn CallsignProvider>, String) {
    match QrzXmlConfig::from_value_provider(|name| values.get(name).cloned()) {
        Ok(config) => match QrzXmlProvider::new(config.clone()) {
            Ok(provider) => {
                let summary = if config.capture_only() {
                    format!("QRZ XML capture-only via {}", config.base_url())
                } else {
                    format!("QRZ XML live via {}", config.base_url())
                };
                (Arc::new(provider), summary)
            }
            Err(error) => {
                let reason = error.to_string();
                (
                    Arc::new(DisabledCallsignProvider::new(reason.clone())),
                    format!("Disabled: {reason}"),
                )
            }
        },
        Err(error) => {
            let reason = error.to_string();
            (
                Arc::new(DisabledCallsignProvider::new(reason.clone())),
                format!("Disabled: {reason}"),
            )
        }
    }
}

fn build_snapshot(
    base_values: &BTreeMap<String, String>,
    session_station_profile_override: Option<&StationProfile>,
    overrides: &BTreeMap<String, String>,
    bindings: &RuntimeBindings,
) -> RuntimeConfigSnapshot {
    let merged = build_effective_values(base_values, session_station_profile_override, overrides);
    let definitions = SUPPORTED_FIELDS
        .iter()
        .map(|field| RuntimeConfigDefinition {
            key: field.key.to_string(),
            label: field.label.to_string(),
            description: field.description.to_string(),
            kind: field.kind as i32,
            secret: field.secret,
            allowed_values: field
                .allowed_values
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        })
        .collect();
    let values = SUPPORTED_FIELDS
        .iter()
        .map(|field| build_value(field, &merged, overrides))
        .collect();
    let mut warnings = Vec::new();

    if overrides.contains_key(STORAGE_BACKEND_ENV_VAR)
        || overrides.contains_key(SQLITE_PATH_ENV_VAR)
    {
        warnings.push(
            "Switching storage backends swaps the active engine state; records are not migrated automatically."
                .to_string(),
        );
    }
    if session_station_profile_override.is_some() {
        warnings.push(
            "A process-session station override is active; new QSOs use it until the override is cleared."
                .to_string(),
        );
    }

    RuntimeConfigSnapshot {
        definitions,
        values,
        active_storage_backend: bindings.active_storage_backend.clone(),
        lookup_provider_summary: bindings.lookup_provider_summary.clone(),
        warnings,
        active_station_profile: bindings.active_station_profile.clone(),
    }
}

fn normalize_optional_runtime_string(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_optional_station_callsign(value: Option<&str>) -> Option<String> {
    normalize_optional_runtime_string(value).map(|value| normalize_callsign(&value))
}

fn build_value(
    field: &ConfigFieldSpec,
    merged: &BTreeMap<String, String>,
    overrides: &BTreeMap<String, String>,
) -> RuntimeConfigValue {
    let effective_value = merged
        .get(field.key)
        .cloned()
        .or_else(|| field.default_value.map(str::to_string));
    let has_value = effective_value.is_some();
    let display_value = if field.secret && has_value {
        REDACTED_VALUE.to_string()
    } else {
        effective_value.unwrap_or_default()
    };

    RuntimeConfigValue {
        key: field.key.to_string(),
        has_value,
        display_value,
        overridden: overrides.contains_key(field.key),
        secret: field.secret,
        redacted: field.secret && has_value,
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{SystemTime, UNIX_EPOCH};

    use qsoripper_core::proto::qsoripper::domain::{Band, Mode, QsoRecord, StationProfile};

    use super::{
        ApplyRuntimeConfigRequest, ResetRuntimeConfigRequest, RuntimeConfigManager,
        RuntimeConfigMutation, RuntimeConfigMutationKind, QRZ_USER_AGENT_ENV_VAR,
        QRZ_XML_CAPTURE_ONLY_ENV_VAR, QRZ_XML_PASSWORD_ENV_VAR, QRZ_XML_USERNAME_ENV_VAR,
        SQLITE_PATH_ENV_VAR, STATION_ARRL_SECTION_ENV_VAR, STATION_CALLSIGN_ENV_VAR,
        STATION_GRID_ENV_VAR, STATION_LATITUDE_ENV_VAR, STATION_OPERATOR_CALLSIGN_ENV_VAR,
        STORAGE_BACKEND_ENV_VAR,
    };

    fn sample_qso(local_id: &str) -> QsoRecord {
        QsoRecord {
            local_id: local_id.to_string(),
            station_callsign: "K7DBG".to_string(),
            worked_callsign: "W1AW".to_string(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: 1_731_600_000,
                nanos: 0,
            }),
            band: Band::Band20m as i32,
            mode: Mode::Ssb as i32,
            ..QsoRecord::default()
        }
    }

    fn unique_sqlite_path() -> String {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "qsoripper-runtime-config-{}-{suffix}.db",
                std::process::id()
            ))
            .display()
            .to_string()
    }

    #[tokio::test]
    async fn runtime_manager_hot_swaps_storage_backends() {
        let manager = RuntimeConfigManager::new(BTreeMap::new()).expect("manager");
        let initial_snapshot = manager.snapshot().await;
        assert_eq!("memory", initial_snapshot.active_storage_backend);

        let sqlite_path = unique_sqlite_path();
        let sqlite_snapshot = manager
            .apply_request(ApplyRuntimeConfigRequest {
                mutations: vec![
                    RuntimeConfigMutation {
                        key: STORAGE_BACKEND_ENV_VAR.to_string(),
                        kind: RuntimeConfigMutationKind::Set as i32,
                        value: Some("sqlite".to_string()),
                    },
                    RuntimeConfigMutation {
                        key: SQLITE_PATH_ENV_VAR.to_string(),
                        kind: RuntimeConfigMutationKind::Set as i32,
                        value: Some(sqlite_path.clone()),
                    },
                ],
            })
            .await
            .expect("sqlite apply");
        assert_eq!("sqlite", sqlite_snapshot.active_storage_backend);

        let sqlite_engine = manager.logbook_engine().await;
        sqlite_engine
            .log_qso(sample_qso("sqlite-record"))
            .await
            .expect("sqlite write");
        drop(sqlite_engine);

        let memory_snapshot = manager
            .apply_request(ApplyRuntimeConfigRequest {
                mutations: vec![RuntimeConfigMutation {
                    key: STORAGE_BACKEND_ENV_VAR.to_string(),
                    kind: RuntimeConfigMutationKind::Set as i32,
                    value: Some("memory".to_string()),
                }],
            })
            .await
            .expect("memory apply");
        assert_eq!("memory", memory_snapshot.active_storage_backend);

        let memory_status = manager
            .logbook_engine()
            .await
            .get_sync_status()
            .await
            .expect("memory status");
        assert_eq!(0, memory_status.local_qso_count);

        let reset_snapshot = manager
            .reset_request(ResetRuntimeConfigRequest {
                keys: vec![
                    STORAGE_BACKEND_ENV_VAR.to_string(),
                    SQLITE_PATH_ENV_VAR.to_string(),
                ],
            })
            .await
            .expect("reset");
        assert_eq!("memory", reset_snapshot.active_storage_backend);
        drop(manager);

        let sqlite_file = std::path::PathBuf::from(sqlite_path);
        if sqlite_file.exists() {
            std::fs::remove_file(sqlite_file).expect("remove sqlite file");
        }
    }

    #[tokio::test]
    async fn runtime_manager_updates_lookup_provider_summary_when_capture_only_changes() {
        let mut base_values = BTreeMap::new();
        base_values.insert(QRZ_XML_USERNAME_ENV_VAR.to_string(), "KC7AVA".to_string());
        base_values.insert(
            QRZ_XML_PASSWORD_ENV_VAR.to_string(),
            "super-secret-password".to_string(),
        );
        base_values.insert(
            QRZ_USER_AGENT_ENV_VAR.to_string(),
            "QsoRipper/0.1.0 (KC7AVA)".to_string(),
        );

        let manager = RuntimeConfigManager::new(base_values).expect("manager");
        let initial_summary = manager.snapshot().await.lookup_provider_summary;
        assert_contains(&initial_summary, "QRZ XML live");

        let capture_only_summary = manager
            .apply_request(ApplyRuntimeConfigRequest {
                mutations: vec![RuntimeConfigMutation {
                    key: QRZ_XML_CAPTURE_ONLY_ENV_VAR.to_string(),
                    kind: RuntimeConfigMutationKind::Set as i32,
                    value: Some("true".to_string()),
                }],
            })
            .await
            .expect("capture-only apply")
            .lookup_provider_summary;
        assert_contains(&capture_only_summary, "capture-only");
    }

    #[tokio::test]
    async fn runtime_manager_exposes_active_station_profile_in_snapshot() {
        let mut base_values = BTreeMap::new();
        base_values.insert(STATION_CALLSIGN_ENV_VAR.to_string(), "K7RND".to_string());
        base_values.insert(
            STATION_OPERATOR_CALLSIGN_ENV_VAR.to_string(),
            "N7OPS".to_string(),
        );
        base_values.insert(STATION_GRID_ENV_VAR.to_string(), "CN87".to_string());
        base_values.insert(STATION_ARRL_SECTION_ENV_VAR.to_string(), "WWA".to_string());

        let manager = RuntimeConfigManager::new(base_values).expect("manager");
        let snapshot = manager.snapshot().await;
        let profile = snapshot.active_station_profile.expect("active profile");

        assert_eq!("K7RND", profile.station_callsign);
        assert_eq!(Some("N7OPS"), profile.operator_callsign.as_deref());
        assert_eq!(Some("CN87"), profile.grid.as_deref());
        assert_eq!(Some("WWA"), profile.arrl_section.as_deref());
    }

    #[tokio::test]
    async fn runtime_manager_prefers_config_file_storage_when_no_cli_override_is_present() {
        let sqlite_path = unique_sqlite_path();
        let mut config_values = BTreeMap::new();
        config_values.insert(STORAGE_BACKEND_ENV_VAR.to_string(), "sqlite".to_string());
        config_values.insert(SQLITE_PATH_ENV_VAR.to_string(), sqlite_path.clone());

        let manager = RuntimeConfigManager::new_with_config_file_values_and_cli_storage_overrides(
            config_values,
            &BTreeMap::new(),
        )
        .expect("manager");

        let snapshot = manager.snapshot().await;
        assert_eq!("sqlite", snapshot.active_storage_backend);
        assert_eq!(
            sqlite_path,
            snapshot
                .values
                .iter()
                .find(|value| value.key == SQLITE_PATH_ENV_VAR)
                .expect("sqlite path value")
                .display_value
        );
    }

    #[tokio::test]
    async fn runtime_manager_prefers_cli_storage_override_over_config_file_storage() {
        let mut config_values = BTreeMap::new();
        config_values.insert(STORAGE_BACKEND_ENV_VAR.to_string(), "sqlite".to_string());
        config_values.insert(SQLITE_PATH_ENV_VAR.to_string(), unique_sqlite_path());

        let mut cli_overrides = BTreeMap::new();
        cli_overrides.insert(STORAGE_BACKEND_ENV_VAR.to_string(), "memory".to_string());

        let manager = RuntimeConfigManager::new_with_config_file_values_and_cli_storage_overrides(
            config_values,
            &cli_overrides,
        )
        .expect("manager");

        let snapshot = manager.snapshot().await;
        assert_eq!("memory", snapshot.active_storage_backend);
    }

    #[tokio::test]
    async fn runtime_manager_applies_station_profile_overrides() {
        let manager = RuntimeConfigManager::new(BTreeMap::new()).expect("manager");

        let snapshot = manager
            .apply_request(ApplyRuntimeConfigRequest {
                mutations: vec![
                    RuntimeConfigMutation {
                        key: STATION_CALLSIGN_ENV_VAR.to_string(),
                        kind: RuntimeConfigMutationKind::Set as i32,
                        value: Some("K7RND".to_string()),
                    },
                    RuntimeConfigMutation {
                        key: STATION_LATITUDE_ENV_VAR.to_string(),
                        kind: RuntimeConfigMutationKind::Set as i32,
                        value: Some("47.6205".to_string()),
                    },
                    RuntimeConfigMutation {
                        key: STATION_ARRL_SECTION_ENV_VAR.to_string(),
                        kind: RuntimeConfigMutationKind::Set as i32,
                        value: Some("WWA".to_string()),
                    },
                ],
            })
            .await
            .expect("station overrides");
        let profile = snapshot.active_station_profile.expect("active profile");

        assert_eq!("K7RND", profile.station_callsign);
        assert_eq!(Some(47.6205), profile.latitude);
        assert_eq!(Some("WWA"), profile.arrl_section.as_deref());
    }

    fn assert_contains(actual: &str, expected: &str) {
        assert!(
            actual.contains(expected),
            "expected '{actual}' to contain '{expected}'"
        );
    }

    #[tokio::test]
    async fn runtime_manager_applies_process_session_station_override() {
        let mut config_values = BTreeMap::new();
        config_values.insert(STATION_CALLSIGN_ENV_VAR.to_string(), "K7RND".to_string());
        config_values.insert(STATION_GRID_ENV_VAR.to_string(), "CN87".to_string());
        let manager =
            RuntimeConfigManager::new_with_config_file_values(BTreeMap::new(), config_values)
                .expect("manager");

        let override_profile = manager
            .set_session_station_profile_override(Some(StationProfile {
                profile_name: Some("POTA".to_string()),
                station_callsign: "K7RND/P".to_string(),
                grid: Some("CN88".to_string()),
                ..StationProfile::default()
            }))
            .await
            .expect("session override")
            .expect("profile");
        assert_eq!("K7RND/P", override_profile.station_callsign);

        let snapshot = manager.snapshot().await;
        assert_eq!(
            Some("K7RND/P"),
            snapshot
                .active_station_profile
                .as_ref()
                .map(|profile| profile.station_callsign.as_str())
        );
        assert!(snapshot
            .warnings
            .iter()
            .any(|warning| warning.contains("process-session")));

        manager
            .set_session_station_profile_override(None)
            .await
            .expect("clear override");
        let cleared = manager.snapshot().await;
        assert_eq!(
            Some("K7RND"),
            cleared
                .active_station_profile
                .as_ref()
                .map(|profile| profile.station_callsign.as_str())
        );
    }
}
