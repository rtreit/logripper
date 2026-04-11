use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use logripper_core::domain::lookup::normalize_callsign;
use logripper_core::domain::station::station_profile_has_values;
use logripper_core::lookup::{QRZ_XML_PASSWORD_ENV_VAR, QRZ_XML_USERNAME_ENV_VAR};
use logripper_core::proto::logripper::domain::StationProfile;
use logripper_core::proto::logripper::services::{
    setup_service_server::SetupService, station_profile_service_server::StationProfileService,
    ClearSessionStationProfileOverrideRequest, ClearSessionStationProfileOverrideResponse,
    DeleteStationProfileRequest, DeleteStationProfileResponse, GetActiveStationContextRequest,
    GetActiveStationContextResponse, GetSetupStatusRequest, GetStationProfileRequest,
    GetStationProfileResponse, ListStationProfilesRequest, ListStationProfilesResponse,
    SaveSetupRequest, SaveSetupResponse, SaveStationProfileRequest, SaveStationProfileResponse,
    SetActiveStationProfileRequest, SetActiveStationProfileResponse,
    SetSessionStationProfileOverrideRequest, SetSessionStationProfileOverrideResponse,
    SetupStatusResponse, StationProfileRecord, StorageBackend,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::runtime_config::{RuntimeConfigManager, SQLITE_PATH_ENV_VAR, STORAGE_BACKEND_ENV_VAR};
use crate::station_profile_support::{
    insert_station_profile_runtime_values, normalize_station_profile as normalize_profile_payload,
    DEFAULT_PROFILE_NAME,
};

pub(crate) const CONFIG_PATH_ENV_VAR: &str = "LOGRIPPER_CONFIG_PATH";
const DEFAULT_CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_SQLITE_FILE_NAME: &str = "logripper.db";

#[derive(Clone)]
pub(crate) struct SetupControlSurface {
    state: Arc<SetupState>,
    runtime_config: Arc<RuntimeConfigManager>,
}

impl SetupControlSurface {
    pub(crate) fn new(state: Arc<SetupState>, runtime_config: Arc<RuntimeConfigManager>) -> Self {
        Self {
            state,
            runtime_config,
        }
    }
}

#[derive(Clone)]
pub(crate) struct StationProfileControlSurface {
    state: Arc<SetupState>,
    runtime_config: Arc<RuntimeConfigManager>,
}

impl StationProfileControlSurface {
    pub(crate) fn new(state: Arc<SetupState>, runtime_config: Arc<RuntimeConfigManager>) -> Self {
        Self {
            state,
            runtime_config,
        }
    }
}

#[tonic::async_trait]
impl SetupService for SetupControlSurface {
    async fn get_setup_status(
        &self,
        _request: Request<GetSetupStatusRequest>,
    ) -> Result<Response<SetupStatusResponse>, Status> {
        Ok(Response::new(self.state.status().await))
    }

    async fn save_setup(
        &self,
        request: Request<SaveSetupRequest>,
    ) -> Result<Response<SaveSetupResponse>, Status> {
        let status = self
            .state
            .save_setup(request.into_inner(), &self.runtime_config)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(SaveSetupResponse {
            status: Some(status),
        }))
    }
}

#[tonic::async_trait]
impl StationProfileService for StationProfileControlSurface {
    async fn list_station_profiles(
        &self,
        _request: Request<ListStationProfilesRequest>,
    ) -> Result<Response<ListStationProfilesResponse>, Status> {
        Ok(Response::new(self.state.list_station_profiles().await))
    }

    async fn get_station_profile(
        &self,
        request: Request<GetStationProfileRequest>,
    ) -> Result<Response<GetStationProfileResponse>, Status> {
        let response = self
            .state
            .get_station_profile(request.into_inner())
            .await
            .map_err(Status::not_found)?;
        Ok(Response::new(response))
    }

    async fn save_station_profile(
        &self,
        request: Request<SaveStationProfileRequest>,
    ) -> Result<Response<SaveStationProfileResponse>, Status> {
        let response = self
            .state
            .save_station_profile(request.into_inner(), &self.runtime_config)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(response))
    }

    async fn delete_station_profile(
        &self,
        request: Request<DeleteStationProfileRequest>,
    ) -> Result<Response<DeleteStationProfileResponse>, Status> {
        let response = self
            .state
            .delete_station_profile(request.into_inner(), &self.runtime_config)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(response))
    }

    async fn set_active_station_profile(
        &self,
        request: Request<SetActiveStationProfileRequest>,
    ) -> Result<Response<SetActiveStationProfileResponse>, Status> {
        let response = self
            .state
            .set_active_station_profile(request.into_inner(), &self.runtime_config)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(response))
    }

    async fn get_active_station_context(
        &self,
        _request: Request<GetActiveStationContextRequest>,
    ) -> Result<Response<GetActiveStationContextResponse>, Status> {
        Ok(Response::new(
            self.state
                .active_station_context(&self.runtime_config)
                .await,
        ))
    }

    async fn set_session_station_profile_override(
        &self,
        request: Request<SetSessionStationProfileOverrideRequest>,
    ) -> Result<Response<SetSessionStationProfileOverrideResponse>, Status> {
        let context = self
            .state
            .set_session_station_profile_override(request.into_inner(), &self.runtime_config)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(SetSessionStationProfileOverrideResponse {
            context: Some(context),
        }))
    }

    async fn clear_session_station_profile_override(
        &self,
        _request: Request<ClearSessionStationProfileOverrideRequest>,
    ) -> Result<Response<ClearSessionStationProfileOverrideResponse>, Status> {
        let context = self
            .state
            .clear_session_station_profile_override(&self.runtime_config)
            .await
            .map_err(Status::invalid_argument)?;
        Ok(Response::new(ClearSessionStationProfileOverrideResponse {
            context: Some(context),
        }))
    }
}

pub(crate) struct SetupState {
    config_path: PathBuf,
    suggested_sqlite_path: PathBuf,
    persisted_config: RwLock<Option<PersistedSetupConfig>>,
}

impl SetupState {
    pub(crate) fn load(config_path: PathBuf) -> Result<Self, String> {
        let persisted_config = load_persisted_config(&config_path)?;
        Ok(Self {
            suggested_sqlite_path: suggested_sqlite_path(&config_path),
            config_path,
            persisted_config: RwLock::new(persisted_config),
        })
    }

    pub(crate) async fn runtime_config_values(&self) -> BTreeMap<String, String> {
        self.persisted_config
            .read()
            .await
            .as_ref()
            .map_or_else(BTreeMap::new, PersistedSetupConfig::to_runtime_values)
    }

    pub(crate) async fn status(&self) -> SetupStatusResponse {
        let persisted_config = self.persisted_config.read().await.clone();
        build_status(
            self.config_path.as_path(),
            self.suggested_sqlite_path.as_path(),
            persisted_config.as_ref(),
        )
    }

    async fn save_setup(
        &self,
        request: SaveSetupRequest,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<SetupStatusResponse, String> {
        let existing_config = self.persisted_config.read().await.clone();
        let config = PersistedSetupConfig::from_request(
            existing_config.as_ref(),
            &request,
            self.suggested_sqlite_path.as_path(),
        )?;
        let runtime_values = config.to_runtime_values();
        runtime_config
            .preview_config_file_values(runtime_values.clone())
            .await?;

        write_persisted_config(self.config_path.as_path(), &config)?;

        {
            let mut persisted_config = self.persisted_config.write().await;
            *persisted_config = Some(config);
        }

        runtime_config
            .replace_config_file_values(runtime_values)
            .await?;
        Ok(self.status().await)
    }

    async fn list_station_profiles(&self) -> ListStationProfilesResponse {
        let persisted_config = self.persisted_config.read().await.clone();
        ListStationProfilesResponse {
            profiles: persisted_config
                .as_ref()
                .map_or_else(Vec::new, PersistedSetupConfig::list_station_profile_records),
            active_profile_id: persisted_config
                .as_ref()
                .and_then(PersistedSetupConfig::active_station_profile_id),
        }
    }

    async fn get_station_profile(
        &self,
        request: GetStationProfileRequest,
    ) -> Result<GetStationProfileResponse, String> {
        let profile_id = normalize_profile_id(&request.profile_id);
        let persisted_config = self.persisted_config.read().await.clone();
        let profile = persisted_config
            .as_ref()
            .and_then(|config| config.get_station_profile_record(&profile_id))
            .ok_or_else(|| format!("Station profile '{profile_id}' was not found."))?;
        Ok(GetStationProfileResponse {
            profile: Some(profile),
        })
    }

    async fn save_station_profile(
        &self,
        request: SaveStationProfileRequest,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<SaveStationProfileResponse, String> {
        let profile = normalize_profile_payload(
            request
                .profile
                .ok_or_else(|| "SaveStationProfile requires a profile payload.".to_string())?,
            normalize_optional_callsign,
            normalize_optional_string,
        )?;
        let profile_id = self
            .mutate_persisted_config(runtime_config, |config| {
                Ok(config.save_station_profile(
                    request.profile_id.as_deref(),
                    &profile,
                    request.make_active,
                ))
            })
            .await?;
        let persisted_config = self.persisted_config.read().await.clone();
        let saved = persisted_config
            .as_ref()
            .and_then(|config| config.get_station_profile_record(&profile_id))
            .ok_or_else(|| format!("Station profile '{profile_id}' was not found after save."))?;
        Ok(SaveStationProfileResponse {
            profile: Some(saved),
            active_profile_id: persisted_config
                .as_ref()
                .and_then(PersistedSetupConfig::active_station_profile_id),
        })
    }

    async fn delete_station_profile(
        &self,
        request: DeleteStationProfileRequest,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<DeleteStationProfileResponse, String> {
        self.mutate_persisted_config(runtime_config, |config| {
            config.delete_station_profile(&request.profile_id)
        })
        .await?;
        let persisted_config = self.persisted_config.read().await.clone();
        Ok(DeleteStationProfileResponse {
            active_profile_id: persisted_config
                .as_ref()
                .and_then(PersistedSetupConfig::active_station_profile_id),
        })
    }

    async fn set_active_station_profile(
        &self,
        request: SetActiveStationProfileRequest,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<SetActiveStationProfileResponse, String> {
        let profile_id = normalize_profile_id(&request.profile_id);
        self.mutate_persisted_config(runtime_config, |config| {
            config.set_active_station_profile(&profile_id)
        })
        .await?;
        let persisted_config = self.persisted_config.read().await.clone();
        let active = persisted_config
            .as_ref()
            .and_then(|config| config.get_station_profile_record(&profile_id))
            .ok_or_else(|| {
                format!("Station profile '{profile_id}' was not found after activation.")
            })?;
        Ok(SetActiveStationProfileResponse {
            profile: Some(active),
        })
    }

    async fn active_station_context(
        &self,
        runtime_config: &RuntimeConfigManager,
    ) -> GetActiveStationContextResponse {
        let persisted_config = self.persisted_config.read().await.clone();
        let session_override_profile = runtime_config.session_station_profile_override().await;
        let effective_active_profile = runtime_config.effective_station_profile().await;
        let mut warnings = Vec::new();
        if persisted_config.is_none() {
            warnings.push("Persisted setup does not exist yet.".to_string());
        }
        if session_override_profile.is_some() {
            warnings.push(
                "A process-session station override is active for new QSO saves.".to_string(),
            );
        }

        GetActiveStationContextResponse {
            persisted_active_profile_id: persisted_config
                .as_ref()
                .and_then(PersistedSetupConfig::active_station_profile_id),
            persisted_active_profile: persisted_config
                .as_ref()
                .and_then(PersistedSetupConfig::station_profile),
            effective_active_profile,
            has_session_override: session_override_profile.is_some(),
            session_override_profile,
            warnings,
        }
    }

    async fn set_session_station_profile_override(
        &self,
        request: SetSessionStationProfileOverrideRequest,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<GetActiveStationContextResponse, String> {
        let profile = normalize_profile_payload(
            request.profile.ok_or_else(|| {
                "SetSessionStationProfileOverride requires a profile payload.".to_string()
            })?,
            normalize_optional_callsign,
            normalize_optional_string,
        )?;
        runtime_config
            .set_session_station_profile_override(Some(profile))
            .await?;
        Ok(self.active_station_context(runtime_config).await)
    }

    async fn clear_session_station_profile_override(
        &self,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<GetActiveStationContextResponse, String> {
        runtime_config
            .set_session_station_profile_override(None)
            .await?;
        Ok(self.active_station_context(runtime_config).await)
    }

    async fn mutate_persisted_config<T>(
        &self,
        runtime_config: &RuntimeConfigManager,
        mutate: impl FnOnce(&mut PersistedSetupConfig) -> Result<T, String>,
    ) -> Result<T, String> {
        let current = self.persisted_config.read().await.clone().ok_or_else(|| {
            "Persisted setup does not exist yet. Run SaveSetup first.".to_string()
        })?;
        let mut next = current;
        let result = mutate(&mut next)?;
        let runtime_values = next.to_runtime_values();
        runtime_config
            .preview_config_file_values(runtime_values.clone())
            .await?;
        write_persisted_config(self.config_path.as_path(), &next)?;
        {
            let mut persisted_config = self.persisted_config.write().await;
            *persisted_config = Some(next);
        }
        runtime_config
            .replace_config_file_values(runtime_values)
            .await?;
        Ok(result)
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedSetupConfig {
    #[serde(default)]
    storage: PersistedStorageConfig,
    #[serde(default)]
    station_profile: PersistedStationProfile,
    #[serde(default)]
    station_profiles: PersistedStationProfileCatalog,
    #[serde(default)]
    qrz_xml: PersistedQrzXmlConfig,
}

impl PersistedSetupConfig {
    fn from_request(
        existing: Option<&Self>,
        request: &SaveSetupRequest,
        suggested_sqlite_path: &Path,
    ) -> Result<Self, String> {
        let storage_backend = StorageBackend::try_from(request.storage_backend)
            .map_err(|_| "A supported storage_backend is required.".to_string())?;
        let station_profile = normalize_profile_payload(
            request
                .station_profile
                .clone()
                .ok_or_else(|| "SaveSetup requires a station_profile payload.".to_string())?,
            normalize_optional_callsign,
            normalize_optional_string,
        )?;
        let qrz_xml_username = normalize_optional_string(request.qrz_xml_username.as_deref());
        let qrz_xml_password = normalize_optional_string(request.qrz_xml_password.as_deref());

        if qrz_xml_username.is_some() != qrz_xml_password.is_some() {
            return Err(
                "QRZ XML username and password must either both be set or both be omitted."
                    .to_string(),
            );
        }

        let sqlite_path = normalize_optional_string(request.sqlite_path.as_deref());
        let storage = match storage_backend {
            StorageBackend::Memory => PersistedStorageConfig {
                backend: Some("memory".to_string()),
                sqlite_path: None,
            },
            StorageBackend::Sqlite => PersistedStorageConfig {
                backend: Some("sqlite".to_string()),
                sqlite_path: Some(
                    sqlite_path.unwrap_or_else(|| suggested_sqlite_path.display().to_string()),
                ),
            },
            StorageBackend::Unspecified => {
                return Err("A supported storage_backend is required.".to_string());
            }
        };

        let mut station_profiles = existing
            .map(|config| config.station_profiles.clone())
            .unwrap_or_default();
        let legacy_profile = existing
            .map(|config| config.station_profile.clone())
            .unwrap_or_default();
        let active_profile_id = existing.and_then(PersistedSetupConfig::active_station_profile_id);
        station_profiles.save_profile(
            active_profile_id.as_deref(),
            &station_profile,
            true,
            &legacy_profile,
        );

        let mut config = existing.cloned().unwrap_or_default();
        config.storage = storage;
        config.station_profile = PersistedStationProfile::from_proto(&station_profile);
        config.station_profiles = station_profiles;
        config.qrz_xml = PersistedQrzXmlConfig {
            username: qrz_xml_username,
            password: qrz_xml_password,
        };
        config.sync_active_station_profile();

        Ok(config)
    }

    fn to_runtime_values(&self) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();

        if let Some(backend) = self.storage.backend.as_deref() {
            values.insert(STORAGE_BACKEND_ENV_VAR.to_string(), backend.to_string());
        }
        if let Some(sqlite_path) = self.storage.sqlite_path.as_deref() {
            values.insert(SQLITE_PATH_ENV_VAR.to_string(), sqlite_path.to_string());
        }

        if let Some(profile) = self.station_profile() {
            insert_station_profile_runtime_values(&mut values, &profile);
        }

        if let Some(username) = self.qrz_xml.username.as_deref() {
            values.insert(QRZ_XML_USERNAME_ENV_VAR.to_string(), username.to_string());
        }
        if let Some(password) = self.qrz_xml.password.as_deref() {
            values.insert(QRZ_XML_PASSWORD_ENV_VAR.to_string(), password.to_string());
        }

        values
    }

    fn storage_backend(&self) -> StorageBackend {
        match self.storage.backend.as_deref() {
            Some("memory") => StorageBackend::Memory,
            Some("sqlite") => StorageBackend::Sqlite,
            _ => StorageBackend::Unspecified,
        }
    }

    fn station_profile(&self) -> Option<StationProfile> {
        let profile = self.station_profiles.active_profile().map_or_else(
            || self.station_profile.to_proto(),
            PersistedStationProfileEntry::to_proto,
        );
        station_profile_has_values(&profile).then_some(profile)
    }

    fn active_station_profile_id(&self) -> Option<String> {
        self.station_profiles.active_profile_id()
    }

    fn station_profile_count(&self) -> usize {
        self.station_profiles
            .count_with_legacy_fallback(&self.station_profile)
    }

    fn list_station_profile_records(&self) -> Vec<StationProfileRecord> {
        self.station_profiles
            .list_records(&self.station_profile)
            .into_iter()
            .map(|entry| entry.to_record(self.active_station_profile_id().as_deref()))
            .collect()
    }

    fn get_station_profile_record(&self, profile_id: &str) -> Option<StationProfileRecord> {
        self.station_profiles
            .list_records(&self.station_profile)
            .into_iter()
            .find(|entry| entry.profile_id == normalize_profile_id(profile_id))
            .map(|entry| entry.to_record(self.active_station_profile_id().as_deref()))
    }

    fn save_station_profile(
        &mut self,
        requested_profile_id: Option<&str>,
        profile: &StationProfile,
        make_active: bool,
    ) -> String {
        let profile_id = self.station_profiles.save_profile(
            requested_profile_id,
            profile,
            make_active,
            &self.station_profile,
        );
        self.sync_active_station_profile();
        profile_id
    }

    fn delete_station_profile(&mut self, profile_id: &str) -> Result<(), String> {
        self.station_profiles
            .delete_profile(profile_id, &self.station_profile)?;
        self.sync_active_station_profile();
        Ok(())
    }

    fn set_active_station_profile(&mut self, profile_id: &str) -> Result<(), String> {
        self.station_profiles
            .set_active_profile(profile_id, &self.station_profile)?;
        self.sync_active_station_profile();
        Ok(())
    }

    fn sync_active_station_profile(&mut self) {
        self.station_profile = self.station_profiles.active_profile().map_or_else(
            || self.station_profile.clone(),
            |entry| PersistedStationProfile::from_proto(&entry.to_proto()),
        );
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedStorageConfig {
    backend: Option<String>,
    sqlite_path: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedStationProfile {
    profile_name: Option<String>,
    station_callsign: Option<String>,
    operator_callsign: Option<String>,
    operator_name: Option<String>,
    grid: Option<String>,
    county: Option<String>,
    state: Option<String>,
    country: Option<String>,
    dxcc: Option<u32>,
    cq_zone: Option<u32>,
    itu_zone: Option<u32>,
    latitude: Option<f64>,
    longitude: Option<f64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedStationProfileCatalog {
    active_profile_id: Option<String>,
    #[serde(default)]
    entries: Vec<PersistedStationProfileEntry>,
}

impl PersistedStationProfileCatalog {
    fn active_profile_id(&self) -> Option<String> {
        self.active_profile()
            .map(|entry| entry.profile_id.clone())
            .or_else(|| self.active_profile_id.clone())
    }

    fn active_profile(&self) -> Option<&PersistedStationProfileEntry> {
        self.active_profile_id
            .as_deref()
            .and_then(|profile_id| self.find_entry(profile_id))
            .or_else(|| self.entries.first())
    }

    fn find_entry(&self, profile_id: &str) -> Option<&PersistedStationProfileEntry> {
        let normalized_id = normalize_profile_id(profile_id);
        self.entries
            .iter()
            .find(|entry| entry.profile_id == normalized_id)
    }

    fn count_with_legacy_fallback(&self, legacy_profile: &PersistedStationProfile) -> usize {
        if self.entries.is_empty() && legacy_profile.has_values() {
            1
        } else {
            self.entries.len()
        }
    }

    fn list_records(
        &self,
        legacy_profile: &PersistedStationProfile,
    ) -> Vec<PersistedStationProfileEntry> {
        if self.entries.is_empty() && legacy_profile.has_values() {
            vec![PersistedStationProfileEntry {
                profile_id: generate_profile_id(None, &legacy_profile.to_proto(), &[]),
                profile: legacy_profile.clone(),
            }]
        } else {
            self.entries.clone()
        }
    }

    fn save_profile(
        &mut self,
        requested_profile_id: Option<&str>,
        profile: &StationProfile,
        make_active: bool,
        legacy_profile: &PersistedStationProfile,
    ) -> String {
        self.bootstrap_from_legacy(legacy_profile);
        let existing_ids: Vec<String> = self
            .entries
            .iter()
            .map(|entry| entry.profile_id.clone())
            .collect();
        let profile_id = requested_profile_id
            .map(normalize_profile_id)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| generate_profile_id(None, profile, &existing_ids));

        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.profile_id == profile_id)
        {
            entry.profile = PersistedStationProfile::from_proto(profile);
        } else {
            self.entries.push(PersistedStationProfileEntry {
                profile_id: profile_id.clone(),
                profile: PersistedStationProfile::from_proto(profile),
            });
        }

        if make_active || self.active_profile_id.as_deref().is_none() {
            self.active_profile_id = Some(profile_id.clone());
        }

        profile_id
    }

    fn delete_profile(
        &mut self,
        profile_id: &str,
        legacy_profile: &PersistedStationProfile,
    ) -> Result<(), String> {
        self.bootstrap_from_legacy(legacy_profile);
        let normalized_id = normalize_profile_id(profile_id);
        if self.active_profile_id.as_deref() == Some(normalized_id.as_str()) {
            return Err(
                "The active station profile cannot be deleted. Activate another profile first."
                    .to_string(),
            );
        }
        let initial_len = self.entries.len();
        self.entries
            .retain(|entry| entry.profile_id != normalized_id);
        if self.entries.len() == initial_len {
            return Err(format!("Station profile '{normalized_id}' was not found."));
        }
        Ok(())
    }

    fn set_active_profile(
        &mut self,
        profile_id: &str,
        legacy_profile: &PersistedStationProfile,
    ) -> Result<(), String> {
        self.bootstrap_from_legacy(legacy_profile);
        let normalized_id = normalize_profile_id(profile_id);
        if self
            .entries
            .iter()
            .any(|entry| entry.profile_id == normalized_id)
        {
            self.active_profile_id = Some(normalized_id);
            Ok(())
        } else {
            Err(format!("Station profile '{normalized_id}' was not found."))
        }
    }

    fn bootstrap_from_legacy(&mut self, legacy_profile: &PersistedStationProfile) {
        if self.entries.is_empty() && legacy_profile.has_values() {
            let profile = legacy_profile.to_proto();
            self.entries.push(PersistedStationProfileEntry {
                profile_id: generate_profile_id(None, &profile, &[]),
                profile: legacy_profile.clone(),
            });
        }
        if self.active_profile_id.as_deref().is_none() {
            self.active_profile_id = self.entries.first().map(|entry| entry.profile_id.clone());
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedStationProfileEntry {
    profile_id: String,
    #[serde(flatten)]
    profile: PersistedStationProfile,
}

impl PersistedStationProfileEntry {
    fn to_proto(&self) -> StationProfile {
        self.profile.to_proto()
    }

    fn to_record(&self, active_profile_id: Option<&str>) -> StationProfileRecord {
        StationProfileRecord {
            profile_id: self.profile_id.clone(),
            profile: Some(self.to_proto()),
            is_active: active_profile_id == Some(self.profile_id.as_str()),
        }
    }
}

impl PersistedStationProfile {
    fn from_proto(profile: &StationProfile) -> Self {
        Self {
            profile_name: normalize_optional_string(profile.profile_name.as_deref()),
            station_callsign: normalize_optional_callsign(Some(profile.station_callsign.as_str())),
            operator_callsign: normalize_optional_callsign(profile.operator_callsign.as_deref()),
            operator_name: normalize_optional_string(profile.operator_name.as_deref()),
            grid: normalize_optional_string(profile.grid.as_deref()),
            county: normalize_optional_string(profile.county.as_deref()),
            state: normalize_optional_string(profile.state.as_deref()),
            country: normalize_optional_string(profile.country.as_deref()),
            dxcc: profile.dxcc,
            cq_zone: profile.cq_zone,
            itu_zone: profile.itu_zone,
            latitude: profile.latitude,
            longitude: profile.longitude,
        }
    }

    fn to_proto(&self) -> StationProfile {
        StationProfile {
            profile_name: self.profile_name.clone(),
            station_callsign: self.station_callsign.clone().unwrap_or_default(),
            operator_callsign: self.operator_callsign.clone(),
            operator_name: self.operator_name.clone(),
            grid: self.grid.clone(),
            county: self.county.clone(),
            state: self.state.clone(),
            country: self.country.clone(),
            dxcc: self.dxcc,
            cq_zone: self.cq_zone,
            itu_zone: self.itu_zone,
            latitude: self.latitude,
            longitude: self.longitude,
        }
    }

    fn has_values(&self) -> bool {
        station_profile_has_values(&self.to_proto())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedQrzXmlConfig {
    username: Option<String>,
    password: Option<String>,
}

pub(crate) fn default_config_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "APPDATA is not set; cannot resolve the default config path.".to_string()
            })?;
        Ok(app_data.join("logripper").join(DEFAULT_CONFIG_FILE_NAME))
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg_config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config_home)
                .join("logripper")
                .join(DEFAULT_CONFIG_FILE_NAME));
        }

        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            "HOME is not set; cannot resolve the default config path.".to_string()
        })?;
        Ok(home
            .join(".config")
            .join("logripper")
            .join(DEFAULT_CONFIG_FILE_NAME))
    }
}

fn suggested_sqlite_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(DEFAULT_SQLITE_FILE_NAME)
}

fn load_persisted_config(config_path: &Path) -> Result<Option<PersistedSetupConfig>, String> {
    if !config_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(config_path)
        .map_err(|error| format!("Failed to read config '{}': {error}", config_path.display()))?;
    let mut config = toml::from_str::<PersistedSetupConfig>(&content).map_err(|error| {
        format!(
            "Failed to parse config '{}': {error}",
            config_path.display()
        )
    })?;
    let legacy_station_profile = config.station_profile.clone();
    config
        .station_profiles
        .bootstrap_from_legacy(&legacy_station_profile);
    config.sync_active_station_profile();
    Ok(Some(config))
}

fn write_persisted_config(config_path: &Path, config: &PersistedSetupConfig) -> Result<(), String> {
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create config directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let content = toml::to_string_pretty(config).map_err(|error| {
        format!(
            "Failed to serialize persisted setup config '{}': {error}",
            config_path.display()
        )
    })?;
    fs::write(config_path, content).map_err(|error| {
        format!(
            "Failed to write config '{}': {error}",
            config_path.display()
        )
    })
}

fn build_status(
    config_path: &Path,
    suggested_sqlite_path: &Path,
    persisted_config: Option<&PersistedSetupConfig>,
) -> SetupStatusResponse {
    let warnings = build_warnings(persisted_config);
    let station_profile = persisted_config.and_then(PersistedSetupConfig::station_profile);
    let storage_backend = persisted_config.map_or(
        StorageBackend::Unspecified,
        PersistedSetupConfig::storage_backend,
    );

    SetupStatusResponse {
        config_file_exists: persisted_config.is_some(),
        setup_complete: persisted_config.is_some() && warnings.is_empty(),
        config_path: config_path.display().to_string(),
        storage_backend: storage_backend as i32,
        sqlite_path: persisted_config.and_then(|config| config.storage.sqlite_path.clone()),
        has_station_profile: station_profile.is_some(),
        station_profile,
        qrz_xml_username: persisted_config.and_then(|config| config.qrz_xml.username.clone()),
        has_qrz_xml_password: persisted_config
            .and_then(|config| config.qrz_xml.password.as_ref())
            .is_some(),
        suggested_sqlite_path: suggested_sqlite_path.display().to_string(),
        warnings,
        active_station_profile_id: persisted_config
            .and_then(PersistedSetupConfig::active_station_profile_id),
        station_profile_count: persisted_config.map_or(0, |config| {
            u32::try_from(config.station_profile_count()).unwrap_or(u32::MAX)
        }),
    }
}

fn build_warnings(persisted_config: Option<&PersistedSetupConfig>) -> Vec<String> {
    let Some(config) = persisted_config else {
        return vec!["No persisted LogRipper setup exists yet.".to_string()];
    };

    let mut warnings = Vec::new();

    match config.storage_backend() {
        StorageBackend::Memory | StorageBackend::Sqlite => {}
        StorageBackend::Unspecified => {
            warnings.push("Persisted setup is missing a supported storage backend.".to_string());
        }
    }

    if matches!(config.storage_backend(), StorageBackend::Sqlite)
        && normalize_optional_string(config.storage.sqlite_path.as_deref()).is_none()
    {
        warnings.push("SQLite storage requires a sqlite_path.".to_string());
    }

    if config.station_profile().is_none() {
        warnings.push("Persisted setup is missing a valid station profile.".to_string());
    }
    if config.station_profile_count() > 0 && config.active_station_profile_id().is_none() {
        warnings
            .push("Persisted setup is missing an active station profile selection.".to_string());
    }

    if config.qrz_xml.username.is_some() != config.qrz_xml.password.is_some() {
        warnings.push(
            "Persisted QRZ XML credentials are incomplete; username and password must be paired."
                .to_string(),
        );
    }

    warnings
}

fn normalize_optional_string(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_optional_callsign(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(normalize_callsign(trimmed))
    }
}

fn normalize_profile_id(raw_value: &str) -> String {
    let mut normalized = String::new();
    let mut previous_was_separator = false;
    for character in raw_value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            normalized.push(character);
            previous_was_separator = false;
        } else if !previous_was_separator {
            normalized.push('-');
            previous_was_separator = true;
        }
    }

    normalized.trim_matches('-').to_string()
}

fn generate_profile_id(
    requested_profile_id: Option<&str>,
    profile: &StationProfile,
    existing_ids: &[String],
) -> String {
    let mut base = requested_profile_id
        .map(normalize_profile_id)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            profile
                .profile_name
                .as_deref()
                .map(normalize_profile_id)
                .filter(|value| !value.is_empty())
        })
        .or_else(|| {
            Some(normalize_profile_id(&profile.station_callsign)).filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| DEFAULT_PROFILE_NAME.to_ascii_lowercase());

    if !existing_ids.iter().any(|existing_id| existing_id == &base) {
        return base;
    }

    let original = base.clone();
    let mut suffix = 2_u32;
    while existing_ids.iter().any(|existing_id| existing_id == &base) {
        base = format!("{original}-{suffix}");
        suffix += 1;
    }

    base
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tonic::Request;

    use super::{
        default_config_path, suggested_sqlite_path, SetupControlSurface, SetupState,
        StationProfileControlSurface, DEFAULT_CONFIG_FILE_NAME,
    };
    use crate::runtime_config::RuntimeConfigManager;
    use logripper_core::proto::logripper::domain::StationProfile;
    use logripper_core::proto::logripper::services::{
        setup_service_server::SetupService, station_profile_service_server::StationProfileService,
        GetActiveStationContextRequest, GetSetupStatusRequest, ListStationProfilesRequest,
        SaveSetupRequest, SaveStationProfileRequest, SetActiveStationProfileRequest,
        SetSessionStationProfileOverrideRequest, StorageBackend,
    };

    fn unique_config_path() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "logripper-setup-test-{}-{suffix}",
                std::process::id()
            ))
            .join(DEFAULT_CONFIG_FILE_NAME)
    }

    #[tokio::test]
    async fn get_setup_status_reports_missing_config() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let status =
            SetupService::get_setup_status(&service, Request::new(GetSetupStatusRequest {}))
                .await
                .expect("status")
                .into_inner();

        assert!(!status.config_file_exists);
        assert!(!status.setup_complete);
        assert_eq!(config_path.display().to_string(), status.config_path);
        assert_eq!(
            suggested_sqlite_path(&config_path).display().to_string(),
            status.suggested_sqlite_path
        );
        assert!(status
            .warnings
            .contains(&"No persisted LogRipper setup exists yet.".to_string()));
    }

    #[tokio::test]
    async fn save_setup_persists_config_and_hot_applies_runtime_values() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Sqlite as i32,
                sqlite_path: None,
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    operator_name: Some("Randy".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: Some("k7rnd".to_string()),
                qrz_xml_password: Some("secret".to_string()),
            }),
        )
        .await
        .expect("save setup")
        .into_inner();

        let status = response.status.expect("status payload");
        let station_profile = status.station_profile.as_ref().expect("station profile");
        assert!(status.config_file_exists);
        assert!(status.setup_complete);
        assert_eq!(StorageBackend::Sqlite as i32, status.storage_backend);
        assert_eq!(Some("Home"), station_profile.profile_name.as_deref());
        assert_eq!("K7RND", station_profile.station_callsign);
        assert!(config_path.exists());

        let runtime_snapshot = runtime_config.snapshot().await;
        assert_eq!("sqlite", runtime_snapshot.active_storage_backend);
        assert_eq!(
            Some("K7RND"),
            runtime_snapshot
                .active_station_profile
                .as_ref()
                .map(|profile| profile.station_callsign.as_str())
        );

        drop(service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        fs::remove_dir_all(config_directory).expect("remove temp config directory");
    }

    #[tokio::test]
    async fn save_setup_preserves_existing_station_profiles() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let setup_service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());
        let station_profile_service =
            StationProfileControlSurface::new(setup_state.clone(), runtime_config.clone());

        SetupService::save_setup(
            &setup_service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Memory as i32,
                sqlite_path: None,
                station_profile: Some(StationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: "k7rnd".to_string(),
                    grid: Some("CN87".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: None,
                qrz_xml_password: None,
            }),
        )
        .await
        .expect("initial setup");

        StationProfileService::save_station_profile(
            &station_profile_service,
            Request::new(SaveStationProfileRequest {
                profile_id: None,
                profile: Some(StationProfile {
                    profile_name: Some("POTA".to_string()),
                    station_callsign: "k7rnd/p".to_string(),
                    grid: Some("CN88".to_string()),
                    ..StationProfile::default()
                }),
                make_active: false,
            }),
        )
        .await
        .expect("save second profile");

        let updated = SetupService::save_setup(
            &setup_service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Sqlite as i32,
                sqlite_path: Some("data\\updated.db".to_string()),
                station_profile: Some(StationProfile {
                    profile_name: Some("Home Debug".to_string()),
                    station_callsign: "k7rnd".to_string(),
                    grid: Some("CN86".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: Some("k7rnd".to_string()),
                qrz_xml_password: Some("secret".to_string()),
            }),
        )
        .await
        .expect("updated setup")
        .into_inner()
        .status
        .expect("status");

        assert_eq!(StorageBackend::Sqlite as i32, updated.storage_backend);
        assert_eq!(Some("home"), updated.active_station_profile_id.as_deref());
        assert_eq!(2, updated.station_profile_count);
        assert_eq!(
            Some("Home Debug"),
            updated
                .station_profile
                .as_ref()
                .and_then(|profile| profile.profile_name.as_deref())
        );
        assert_eq!(
            Some("CN86"),
            updated
                .station_profile
                .as_ref()
                .and_then(|profile| profile.grid.as_deref())
        );

        let listed = StationProfileService::list_station_profiles(
            &station_profile_service,
            Request::new(ListStationProfilesRequest {}),
        )
        .await
        .expect("list profiles")
        .into_inner();
        assert_eq!(2, listed.profiles.len());
        assert!(listed
            .profiles
            .iter()
            .any(|profile| profile.profile_id == "pota"));
        assert!(listed.profiles.iter().any(|profile| {
            profile.profile_id == "home"
                && profile
                    .profile
                    .as_ref()
                    .and_then(|value| value.profile_name.as_deref())
                    == Some("Home Debug")
        }));

        let runtime_snapshot = runtime_config.snapshot().await;
        assert_eq!("sqlite", runtime_snapshot.active_storage_backend);

        let config_directory = config_path.parent().expect("config directory");
        fs::remove_dir_all(config_directory).expect("remove temp config directory");
    }

    #[tokio::test]
    async fn save_setup_rejects_partial_qrz_credentials() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let error = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Memory as i32,
                sqlite_path: None,
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                qrz_xml_username: Some("k7rnd".to_string()),
                qrz_xml_password: None,
            }),
        )
        .await
        .expect_err("save setup should fail");

        assert_eq!(tonic::Code::InvalidArgument, error.code());
    }

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn station_profile_service_lists_legacy_profile_and_supports_activation_and_session_override(
    ) {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let setup_service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());
        let station_profile_service =
            StationProfileControlSurface::new(setup_state.clone(), runtime_config.clone());

        SetupService::save_setup(
            &setup_service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Memory as i32,
                sqlite_path: None,
                station_profile: Some(StationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: "k7rnd".to_string(),
                    grid: Some("CN87".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: None,
                qrz_xml_password: None,
            }),
        )
        .await
        .expect("save setup");

        let saved = StationProfileService::save_station_profile(
            &station_profile_service,
            Request::new(SaveStationProfileRequest {
                profile_id: None,
                profile: Some(StationProfile {
                    profile_name: Some("POTA".to_string()),
                    station_callsign: "k7rnd/p".to_string(),
                    grid: Some("CN88".to_string()),
                    ..StationProfile::default()
                }),
                make_active: false,
            }),
        )
        .await
        .expect("save station profile")
        .into_inner();

        assert_eq!(Some("home"), saved.active_profile_id.as_deref());

        let listed = StationProfileService::list_station_profiles(
            &station_profile_service,
            Request::new(ListStationProfilesRequest {}),
        )
        .await
        .expect("list profiles")
        .into_inner();
        assert_eq!(2, listed.profiles.len());
        assert_eq!(Some("home"), listed.active_profile_id.as_deref());
        let portable = listed
            .profiles
            .iter()
            .find(|profile| profile.profile_id == "pota")
            .expect("portable profile");
        assert!(!portable.is_active);

        let activated = StationProfileService::set_active_station_profile(
            &station_profile_service,
            Request::new(SetActiveStationProfileRequest {
                profile_id: "pota".to_string(),
            }),
        )
        .await
        .expect("activate profile")
        .into_inner();
        assert_eq!("pota", activated.profile.expect("profile").profile_id);

        let context = StationProfileService::get_active_station_context(
            &station_profile_service,
            Request::new(GetActiveStationContextRequest {}),
        )
        .await
        .expect("active context")
        .into_inner();
        assert_eq!(Some("pota"), context.persisted_active_profile_id.as_deref());
        assert_eq!(
            Some("K7RND/P"),
            context
                .effective_active_profile
                .as_ref()
                .map(|profile| profile.station_callsign.as_str())
        );

        let override_context = StationProfileService::set_session_station_profile_override(
            &station_profile_service,
            Request::new(SetSessionStationProfileOverrideRequest {
                profile: Some(StationProfile {
                    profile_name: Some("Field Day".to_string()),
                    station_callsign: "k7rnd/7".to_string(),
                    grid: Some("CN85".to_string()),
                    ..StationProfile::default()
                }),
            }),
        )
        .await
        .expect("session override")
        .into_inner()
        .context
        .expect("context");
        assert!(override_context.has_session_override);
        assert_eq!(
            Some("K7RND/7"),
            override_context
                .effective_active_profile
                .as_ref()
                .map(|profile| profile.station_callsign.as_str())
        );

        let config_directory = config_path.parent().expect("config directory");
        fs::remove_dir_all(config_directory).expect("remove temp config directory");
    }

    #[test]
    fn default_config_path_ends_with_standard_filename() {
        let path = default_config_path().expect("default config path");

        assert_eq!(
            Some(DEFAULT_CONFIG_FILE_NAME),
            path.file_name().and_then(|name| name.to_str())
        );
    }
}
