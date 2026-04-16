use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use qsoripper_core::domain::lookup::normalize_callsign;
use qsoripper_core::domain::station::station_profile_has_values;
use qsoripper_core::lookup::{
    QrzXmlConfig, QrzXmlProvider, QRZ_USER_AGENT_ENV_VAR, QRZ_XML_PASSWORD_ENV_VAR,
    QRZ_XML_USERNAME_ENV_VAR,
};
use qsoripper_core::proto::qsoripper::domain::{ConflictPolicy, StationProfile, SyncConfig};
use qsoripper_core::proto::qsoripper::services::{
    setup_service_server::SetupService, station_profile_service_server::StationProfileService,
    ActiveStationContext, ClearSessionStationProfileOverrideRequest,
    ClearSessionStationProfileOverrideResponse, DeleteStationProfileRequest,
    DeleteStationProfileResponse, GetActiveStationContextRequest, GetActiveStationContextResponse,
    GetSetupStatusRequest, GetSetupStatusResponse, GetSetupWizardStateRequest,
    GetSetupWizardStateResponse, GetStationProfileRequest, GetStationProfileResponse,
    ListStationProfilesRequest, ListStationProfilesResponse, RigControlSettings, SaveSetupRequest,
    SaveSetupResponse, SaveStationProfileRequest, SaveStationProfileResponse,
    SetActiveStationProfileRequest, SetActiveStationProfileResponse,
    SetSessionStationProfileOverrideRequest, SetSessionStationProfileOverrideResponse,
    SetupFieldValidation, SetupStatus, SetupWizardStep, SetupWizardStepStatus,
    StationProfileRecord, StorageBackend, TestQrzCredentialsRequest, TestQrzCredentialsResponse,
    TestQrzLogbookCredentialsRequest, TestQrzLogbookCredentialsResponse, ValidateSetupStepRequest,
    ValidateSetupStepResponse,
};
use qsoripper_core::qrz_logbook::{QrzLogbookClient, QrzLogbookConfig};
use qsoripper_core::rig_control::{
    RIGCTLD_ENABLED_ENV_VAR, RIGCTLD_HOST_ENV_VAR, RIGCTLD_PORT_ENV_VAR,
    RIGCTLD_READ_TIMEOUT_MS_ENV_VAR, RIGCTLD_STALE_THRESHOLD_MS_ENV_VAR,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tonic::{Request, Response, Status};

use crate::runtime_config::{
    RuntimeConfigManager, DEFAULT_QRZ_LOGBOOK_BASE_URL, QRZ_LOGBOOK_API_KEY_ENV_VAR,
    QRZ_LOGBOOK_BASE_URL_ENV_VAR, SQLITE_PATH_ENV_VAR, STORAGE_BACKEND_ENV_VAR,
    SYNC_AUTO_ENABLED_ENV_VAR, SYNC_CONFLICT_POLICY_ENV_VAR, SYNC_INTERVAL_SECONDS_ENV_VAR,
};
use crate::station_profile_support::{
    insert_station_profile_runtime_values, normalize_station_profile as normalize_profile_payload,
    DEFAULT_PROFILE_NAME,
};

pub(crate) const CONFIG_PATH_ENV_VAR: &str = "QSORIPPER_CONFIG_PATH";
const DEFAULT_CONFIG_FILE_NAME: &str = "config.toml";
const DEFAULT_LOG_FILE_NAME: &str = "qsoripper.db";

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
    ) -> Result<Response<GetSetupStatusResponse>, Status> {
        Ok(Response::new(GetSetupStatusResponse {
            status: Some(self.state.status().await),
        }))
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

    async fn get_setup_wizard_state(
        &self,
        _request: Request<GetSetupWizardStateRequest>,
    ) -> Result<Response<GetSetupWizardStateResponse>, Status> {
        Ok(Response::new(self.state.wizard_state().await))
    }

    async fn validate_setup_step(
        &self,
        request: Request<ValidateSetupStepRequest>,
    ) -> Result<Response<ValidateSetupStepResponse>, Status> {
        let inner = request.into_inner();
        let step = SetupWizardStep::try_from(inner.step).unwrap_or(SetupWizardStep::Unspecified);
        Ok(Response::new(validate_step(step, &inner)))
    }

    async fn test_qrz_credentials(
        &self,
        request: Request<TestQrzCredentialsRequest>,
    ) -> Result<Response<TestQrzCredentialsResponse>, Status> {
        let inner = request.into_inner();
        let result = test_qrz_login(
            &inner.qrz_xml_username,
            &inner.qrz_xml_password,
            &self.runtime_config,
        )
        .await;
        Ok(Response::new(result))
    }

    async fn test_qrz_logbook_credentials(
        &self,
        request: Request<TestQrzLogbookCredentialsRequest>,
    ) -> Result<Response<TestQrzLogbookCredentialsResponse>, Status> {
        let inner = request.into_inner();
        let result = test_qrz_logbook_api_key(&inner.api_key, &self.runtime_config).await;
        Ok(Response::new(result))
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
        let context = self
            .state
            .active_station_context(&self.runtime_config)
            .await;
        Ok(Response::new(GetActiveStationContextResponse {
            context: Some(context),
        }))
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
    suggested_log_file_path: PathBuf,
    persisted_config: RwLock<Option<PersistedSetupConfig>>,
}

impl SetupState {
    pub(crate) fn load(config_path: PathBuf) -> Result<Self, String> {
        let persisted_config = load_persisted_config(&config_path)?;
        Ok(Self {
            suggested_log_file_path: suggested_log_file_path(&config_path),
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

    pub(crate) async fn status(&self) -> SetupStatus {
        let persisted_config = self.persisted_config.read().await.clone();
        build_status(
            self.config_path.as_path(),
            self.suggested_log_file_path.as_path(),
            persisted_config.as_ref(),
        )
    }

    async fn wizard_state(&self) -> GetSetupWizardStateResponse {
        let persisted_config = self.persisted_config.read().await.clone();
        let status = build_status(
            self.config_path.as_path(),
            self.suggested_log_file_path.as_path(),
            persisted_config.as_ref(),
        );
        let steps = build_wizard_steps(persisted_config.as_ref());
        let station_profiles = persisted_config
            .as_ref()
            .map_or_else(Vec::new, PersistedSetupConfig::list_station_profile_records);
        GetSetupWizardStateResponse {
            status: Some(status),
            steps,
            station_profiles,
        }
    }

    async fn save_setup(
        &self,
        request: SaveSetupRequest,
        runtime_config: &RuntimeConfigManager,
    ) -> Result<SetupStatus, String> {
        let existing_config = self.persisted_config.read().await.clone();
        let config = PersistedSetupConfig::from_request(
            existing_config.as_ref(),
            &request,
            self.suggested_log_file_path.as_path(),
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
    ) -> ActiveStationContext {
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

        ActiveStationContext {
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
    ) -> Result<ActiveStationContext, String> {
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
    ) -> Result<ActiveStationContext, String> {
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
    #[serde(default, skip_serializing_if = "PersistedLogbookConfig::is_empty")]
    logbook: PersistedLogbookConfig,
    #[serde(default, skip_serializing_if = "PersistedStorageConfig::is_empty")]
    storage: PersistedStorageConfig,
    #[serde(default)]
    station_profile: PersistedStationProfile,
    #[serde(default)]
    station_profiles: PersistedStationProfileCatalog,
    #[serde(default)]
    qrz_xml: PersistedQrzXmlConfig,
    #[serde(default, skip_serializing_if = "PersistedQrzLogbookConfig::is_empty")]
    qrz_logbook: PersistedQrzLogbookConfig,
    #[serde(default, skip_serializing_if = "PersistedSyncConfig::is_empty")]
    sync: PersistedSyncConfig,
    #[serde(default, skip_serializing_if = "PersistedRigControlConfig::is_empty")]
    rig_control: PersistedRigControlConfig,
}

impl PersistedSetupConfig {
    fn from_request(
        existing: Option<&Self>,
        request: &SaveSetupRequest,
        suggested_log_file_path: &Path,
    ) -> Result<Self, String> {
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

        let requested_log_file_path = normalize_optional_string(request.log_file_path.as_deref());
        #[allow(deprecated)]
        let legacy_sqlite_path = normalize_optional_string(request.sqlite_path.as_deref());
        #[allow(deprecated)]
        let legacy_storage_backend = StorageBackend::try_from(request.storage_backend)
            .unwrap_or(StorageBackend::Unspecified);
        let (logbook, storage) = if let Some(log_file_path) = requested_log_file_path {
            (
                PersistedLogbookConfig {
                    file_path: Some(log_file_path),
                },
                PersistedStorageConfig::default(),
            )
        } else if matches!(legacy_storage_backend, StorageBackend::Memory) {
            (
                PersistedLogbookConfig::default(),
                PersistedStorageConfig {
                    backend: Some("memory".to_string()),
                    sqlite_path: None,
                },
            )
        } else if matches!(legacy_storage_backend, StorageBackend::Sqlite)
            || legacy_sqlite_path.is_some()
        {
            (
                PersistedLogbookConfig {
                    file_path: Some(
                        legacy_sqlite_path
                            .unwrap_or_else(|| suggested_log_file_path.display().to_string()),
                    ),
                },
                PersistedStorageConfig::default(),
            )
        } else {
            return Err("A log_file_path is required.".to_string());
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
        config.logbook = logbook;
        config.storage = storage;
        config.station_profile = PersistedStationProfile::from_proto(&station_profile);
        config.station_profiles = station_profiles;
        config.qrz_xml = PersistedQrzXmlConfig {
            username: qrz_xml_username,
            password: qrz_xml_password,
            // Preserve any existing user_agent; the setup wizard does not set it
            // directly, and runtime derives a default from the username when absent.
            user_agent: existing.and_then(|c| c.qrz_xml.user_agent.clone()),
        };

        // QRZ logbook API key: update when explicitly provided, otherwise keep existing.
        let qrz_logbook_api_key = normalize_optional_string(request.qrz_logbook_api_key.as_deref());
        if qrz_logbook_api_key.is_some() {
            config.qrz_logbook.api_key = qrz_logbook_api_key;
        }

        // Sync config: update when explicitly provided, otherwise keep existing.
        if let Some(ref sync_config) = request.sync_config {
            config.sync = PersistedSyncConfig::from_proto(sync_config);
        }

        // Rig control config: update when explicitly provided, otherwise keep existing.
        if let Some(ref rig_control) = request.rig_control {
            config.rig_control = PersistedRigControlConfig::from_proto(rig_control)?;
        }

        config.sync_active_station_profile();

        Ok(config)
    }

    fn to_runtime_values(&self) -> BTreeMap<String, String> {
        let mut values = BTreeMap::new();
        let log_file_path = self.log_file_path();

        match self.runtime_storage_backend() {
            StorageBackend::Memory => {
                values.insert(STORAGE_BACKEND_ENV_VAR.to_string(), "memory".to_string());
            }
            StorageBackend::Sqlite => {
                values.insert(STORAGE_BACKEND_ENV_VAR.to_string(), "sqlite".to_string());
                if let Some(log_file_path) = log_file_path {
                    values.insert(SQLITE_PATH_ENV_VAR.to_string(), log_file_path);
                }
            }
            StorageBackend::Unspecified => {}
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
        if let Some(user_agent) = self.qrz_xml.user_agent.as_deref() {
            values.insert(QRZ_USER_AGENT_ENV_VAR.to_string(), user_agent.to_string());
        }

        // QRZ logbook config
        if let Some(api_key) = self.qrz_logbook.api_key.as_deref() {
            values.insert(QRZ_LOGBOOK_API_KEY_ENV_VAR.to_string(), api_key.to_string());
        }
        if let Some(base_url) = self.qrz_logbook.base_url.as_deref() {
            values.insert(
                QRZ_LOGBOOK_BASE_URL_ENV_VAR.to_string(),
                base_url.to_string(),
            );
        }

        // Sync config
        values.insert(
            SYNC_AUTO_ENABLED_ENV_VAR.to_string(),
            self.sync.auto_sync_enabled.to_string(),
        );
        values.insert(
            SYNC_INTERVAL_SECONDS_ENV_VAR.to_string(),
            self.sync.sync_interval_seconds.to_string(),
        );
        if !self.sync.conflict_policy.is_empty() {
            values.insert(
                SYNC_CONFLICT_POLICY_ENV_VAR.to_string(),
                self.sync.conflict_policy.clone(),
            );
        }

        // Rig control config
        if let Some(enabled) = self.rig_control.enabled {
            values.insert(RIGCTLD_ENABLED_ENV_VAR.to_string(), enabled.to_string());
        }
        if let Some(ref host) = self.rig_control.host {
            values.insert(RIGCTLD_HOST_ENV_VAR.to_string(), host.clone());
        }
        if let Some(port) = self.rig_control.port {
            values.insert(RIGCTLD_PORT_ENV_VAR.to_string(), port.to_string());
        }
        if let Some(read_timeout_ms) = self.rig_control.read_timeout_ms {
            values.insert(
                RIGCTLD_READ_TIMEOUT_MS_ENV_VAR.to_string(),
                read_timeout_ms.to_string(),
            );
        }
        if let Some(stale_threshold_ms) = self.rig_control.stale_threshold_ms {
            values.insert(
                RIGCTLD_STALE_THRESHOLD_MS_ENV_VAR.to_string(),
                stale_threshold_ms.to_string(),
            );
        }

        values
    }

    fn log_file_path(&self) -> Option<String> {
        normalize_optional_string(self.logbook.file_path.as_deref())
            .or_else(|| normalize_optional_string(self.storage.sqlite_path.as_deref()))
    }

    fn runtime_storage_backend(&self) -> StorageBackend {
        if self.log_file_path().is_some() {
            StorageBackend::Sqlite
        } else {
            self.legacy_storage_backend()
        }
    }

    fn legacy_storage_backend(&self) -> StorageBackend {
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
struct PersistedLogbookConfig {
    file_path: Option<String>,
}

impl PersistedLogbookConfig {
    fn is_empty(config: &Self) -> bool {
        normalize_optional_string(config.file_path.as_deref()).is_none()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedStorageConfig {
    backend: Option<String>,
    sqlite_path: Option<String>,
}

impl PersistedStorageConfig {
    fn is_empty(config: &Self) -> bool {
        config.backend.is_none()
            && normalize_optional_string(config.sqlite_path.as_deref()).is_none()
    }
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
    arrl_section: Option<String>,
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
            arrl_section: normalize_optional_string(profile.arrl_section.as_deref()),
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
            arrl_section: self.arrl_section.clone(),
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
    user_agent: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedQrzLogbookConfig {
    api_key: Option<String>,
    base_url: Option<String>,
}

impl PersistedQrzLogbookConfig {
    fn is_empty(config: &Self) -> bool {
        normalize_optional_string(config.api_key.as_deref()).is_none()
            && normalize_optional_string(config.base_url.as_deref()).is_none()
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedSyncConfig {
    #[serde(default)]
    auto_sync_enabled: bool,
    #[serde(default = "default_sync_interval_seconds")]
    sync_interval_seconds: u32,
    #[serde(default)]
    conflict_policy: String,
}

fn default_sync_interval_seconds() -> u32 {
    300
}

impl PersistedSyncConfig {
    fn is_empty(config: &Self) -> bool {
        !config.auto_sync_enabled
            && config.sync_interval_seconds == default_sync_interval_seconds()
            && (config.conflict_policy.is_empty() || config.conflict_policy == "last_write_wins")
    }

    fn from_proto(sync_config: &SyncConfig) -> Self {
        let conflict_policy = match ConflictPolicy::try_from(sync_config.conflict_policy) {
            Ok(ConflictPolicy::FlagForReview) => "flag_for_review".to_string(),
            _ => "last_write_wins".to_string(),
        };
        Self {
            auto_sync_enabled: sync_config.auto_sync_enabled,
            sync_interval_seconds: if sync_config.sync_interval_seconds == 0 {
                default_sync_interval_seconds()
            } else {
                sync_config.sync_interval_seconds
            },
            conflict_policy,
        }
    }

    fn to_proto(&self) -> SyncConfig {
        let conflict_policy = match self.conflict_policy.as_str() {
            "flag_for_review" => ConflictPolicy::FlagForReview,
            _ => ConflictPolicy::LastWriteWins,
        };
        SyncConfig {
            auto_sync_enabled: self.auto_sync_enabled,
            sync_interval_seconds: if self.sync_interval_seconds == 0 {
                default_sync_interval_seconds()
            } else {
                self.sync_interval_seconds
            },
            conflict_policy: conflict_policy as i32,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct PersistedRigControlConfig {
    enabled: Option<bool>,
    host: Option<String>,
    port: Option<u16>,
    read_timeout_ms: Option<u64>,
    stale_threshold_ms: Option<u64>,
}

impl PersistedRigControlConfig {
    fn is_empty(config: &Self) -> bool {
        config.enabled.is_none()
            && config.host.is_none()
            && config.port.is_none()
            && config.read_timeout_ms.is_none()
            && config.stale_threshold_ms.is_none()
    }

    fn from_proto(rig_control: &RigControlSettings) -> Result<Self, String> {
        let port = match rig_control.port {
            Some(0) => {
                return Err("Rig control port must be between 1 and 65535.".to_string());
            }
            Some(port) => Some(
                u16::try_from(port)
                    .map_err(|_| "Rig control port must be between 1 and 65535.".to_string())?,
            ),
            None => None,
        };

        let read_timeout_ms = match rig_control.read_timeout_ms {
            Some(0) => {
                return Err(
                    "Rig control read timeout must be greater than 0 milliseconds.".to_string(),
                );
            }
            Some(value) => Some(value),
            None => None,
        };

        let stale_threshold_ms = match rig_control.stale_threshold_ms {
            Some(0) => {
                return Err(
                    "Rig control stale threshold must be greater than 0 milliseconds.".to_string(),
                );
            }
            Some(value) => Some(value),
            None => None,
        };

        Ok(Self {
            enabled: rig_control.enabled,
            host: normalize_optional_string(rig_control.host.as_deref()),
            port,
            read_timeout_ms,
            stale_threshold_ms,
        })
    }

    fn to_proto(&self) -> Option<RigControlSettings> {
        if Self::is_empty(self) {
            return None;
        }

        Some(RigControlSettings {
            enabled: self.enabled,
            host: self.host.clone(),
            port: self.port.map(u32::from),
            read_timeout_ms: self.read_timeout_ms,
            stale_threshold_ms: self.stale_threshold_ms,
        })
    }
}

pub(crate) fn default_config_path() -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| {
                "APPDATA is not set; cannot resolve the default config path.".to_string()
            })?;
        Ok(app_data.join("qsoripper").join(DEFAULT_CONFIG_FILE_NAME))
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(xdg_config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            return Ok(PathBuf::from(xdg_config_home)
                .join("qsoripper")
                .join(DEFAULT_CONFIG_FILE_NAME));
        }

        let home = std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| {
            "HOME is not set; cannot resolve the default config path.".to_string()
        })?;
        Ok(home
            .join(".config")
            .join("qsoripper")
            .join(DEFAULT_CONFIG_FILE_NAME))
    }
}

fn suggested_log_file_path(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(DEFAULT_LOG_FILE_NAME)
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
    suggested_log_file_path: &Path,
    persisted_config: Option<&PersistedSetupConfig>,
) -> SetupStatus {
    let warnings = build_warnings(persisted_config);
    let station_profile = persisted_config.and_then(PersistedSetupConfig::station_profile);
    let log_file_path = persisted_config.and_then(PersistedSetupConfig::log_file_path);
    let storage_backend = persisted_config.map_or(
        StorageBackend::Unspecified,
        PersistedSetupConfig::runtime_storage_backend,
    );

    #[allow(deprecated)]
    SetupStatus {
        config_file_exists: persisted_config.is_some(),
        setup_complete: persisted_config.is_some() && warnings.is_empty(),
        config_path: config_path.display().to_string(),
        storage_backend: storage_backend as i32,
        sqlite_path: log_file_path.clone(),
        has_station_profile: station_profile.is_some(),
        station_profile,
        qrz_xml_username: persisted_config.and_then(|config| config.qrz_xml.username.clone()),
        has_qrz_xml_password: persisted_config
            .and_then(|config| config.qrz_xml.password.as_ref())
            .is_some(),
        suggested_sqlite_path: suggested_log_file_path.display().to_string(),
        warnings,
        active_station_profile_id: persisted_config
            .and_then(PersistedSetupConfig::active_station_profile_id),
        station_profile_count: persisted_config.map_or(0, |config| {
            u32::try_from(config.station_profile_count()).unwrap_or(u32::MAX)
        }),
        log_file_path,
        suggested_log_file_path: suggested_log_file_path.display().to_string(),
        is_first_run: persisted_config.is_none(),
        has_qrz_logbook_api_key: persisted_config
            .and_then(|config| config.qrz_logbook.api_key.as_ref())
            .is_some(),
        sync_config: persisted_config.map(|config| config.sync.to_proto()),
        rig_control: persisted_config.and_then(|config| config.rig_control.to_proto()),
    }
}

fn build_warnings(persisted_config: Option<&PersistedSetupConfig>) -> Vec<String> {
    let Some(config) = persisted_config else {
        return vec!["No persisted QsoRipper setup exists yet.".to_string()];
    };

    let mut warnings = Vec::new();
    let log_file_path = config.log_file_path();

    if log_file_path.is_none() {
        if matches!(config.legacy_storage_backend(), StorageBackend::Memory) {
            warnings.push(
                "Persisted setup still uses legacy in-memory storage; save a log file path to migrate to the backend-agnostic setup model."
                    .to_string(),
            );
        } else {
            warnings.push("Persisted setup is missing a log_file_path.".to_string());
        }
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

fn build_wizard_steps(
    persisted_config: Option<&PersistedSetupConfig>,
) -> Vec<SetupWizardStepStatus> {
    vec![
        build_log_file_step(persisted_config),
        build_station_profiles_step(persisted_config),
        build_qrz_integration_step(persisted_config),
        build_review_step(persisted_config),
    ]
}

fn build_log_file_step(config: Option<&PersistedSetupConfig>) -> SetupWizardStepStatus {
    let log_file = config.and_then(PersistedSetupConfig::log_file_path);
    let complete = log_file.is_some();
    let issues = if complete {
        Vec::new()
    } else {
        vec!["A log file path is required.".to_string()]
    };
    SetupWizardStepStatus {
        step: SetupWizardStep::LogFile.into(),
        complete,
        issues,
    }
}

fn build_station_profiles_step(config: Option<&PersistedSetupConfig>) -> SetupWizardStepStatus {
    let has_profiles = config.is_some_and(|c| c.station_profile_count() > 0);
    let has_active = config
        .and_then(PersistedSetupConfig::active_station_profile_id)
        .is_some();
    let complete = has_profiles && has_active;
    let mut issues = Vec::new();
    if !has_profiles {
        issues.push("At least one station profile is required.".to_string());
    }
    if has_profiles && !has_active {
        issues.push("An active station profile must be selected.".to_string());
    }
    SetupWizardStepStatus {
        step: SetupWizardStep::StationProfiles.into(),
        complete,
        issues,
    }
}

fn build_qrz_integration_step(config: Option<&PersistedSetupConfig>) -> SetupWizardStepStatus {
    let username = config.and_then(|c| c.qrz_xml.username.as_ref());
    let password = config.and_then(|c| c.qrz_xml.password.as_ref());
    // QRZ is optional — step is complete if either both are set or both are absent.
    let paired = username.is_some() == password.is_some();
    let issues = if paired {
        Vec::new()
    } else {
        vec![
            "QRZ XML username and password must either both be set or both be omitted.".to_string(),
        ]
    };
    SetupWizardStepStatus {
        step: SetupWizardStep::QrzIntegration.into(),
        complete: paired,
        issues,
    }
}

fn build_review_step(config: Option<&PersistedSetupConfig>) -> SetupWizardStepStatus {
    let all_prior_complete = config.is_some()
        && config
            .and_then(PersistedSetupConfig::log_file_path)
            .is_some()
        && config.is_some_and(|c| c.station_profile_count() > 0)
        && config
            .and_then(PersistedSetupConfig::active_station_profile_id)
            .is_some();
    let issues = if all_prior_complete {
        Vec::new()
    } else {
        vec!["Complete the previous steps before reviewing.".to_string()]
    };
    SetupWizardStepStatus {
        step: SetupWizardStep::Review.into(),
        complete: all_prior_complete,
        issues,
    }
}

fn validate_step(
    step: SetupWizardStep,
    request: &ValidateSetupStepRequest,
) -> ValidateSetupStepResponse {
    match step {
        SetupWizardStep::LogFile => validate_log_file_step(request),
        SetupWizardStep::StationProfiles => validate_station_profiles_step(request),
        SetupWizardStep::QrzIntegration => validate_qrz_step(request),
        SetupWizardStep::Review | SetupWizardStep::Unspecified => ValidateSetupStepResponse {
            valid: true,
            fields: Vec::new(),
        },
    }
}

fn validate_log_file_step(request: &ValidateSetupStepRequest) -> ValidateSetupStepResponse {
    let path = normalize_optional_string(request.log_file_path.as_deref());
    let (valid, message) = match &path {
        Some(p) => {
            let parent = Path::new(p.as_str()).parent();
            match parent {
                Some(d) if !d.as_os_str().is_empty() && !d.exists() => (
                    false,
                    format!("Parent directory '{}' does not exist.", d.display()),
                ),
                _ => (true, String::new()),
            }
        }
        None => (false, "A log file path is required.".to_string()),
    };
    ValidateSetupStepResponse {
        valid,
        fields: vec![SetupFieldValidation {
            field: "log_file_path".to_string(),
            valid,
            message,
        }],
    }
}

fn validate_station_profiles_step(request: &ValidateSetupStepRequest) -> ValidateSetupStepResponse {
    let profile = request.station_profile.as_ref();
    let mut fields = Vec::new();

    let callsign_valid = profile
        .and_then(|p| normalize_optional_string(Some(p.station_callsign.as_str())))
        .is_some();
    fields.push(SetupFieldValidation {
        field: "station_callsign".to_string(),
        valid: callsign_valid,
        message: if callsign_valid {
            String::new()
        } else {
            "Station callsign is required.".to_string()
        },
    });

    let name_valid = profile
        .and_then(|p| normalize_optional_string(p.profile_name.as_deref()))
        .is_some();
    fields.push(SetupFieldValidation {
        field: "profile_name".to_string(),
        valid: name_valid,
        message: if name_valid {
            String::new()
        } else {
            "Profile name is required.".to_string()
        },
    });

    let operator_valid = profile
        .and_then(|p| normalize_optional_string(p.operator_callsign.as_deref()))
        .is_some();
    fields.push(SetupFieldValidation {
        field: "operator_callsign".to_string(),
        valid: operator_valid,
        message: if operator_valid {
            String::new()
        } else {
            "Operator callsign is required.".to_string()
        },
    });

    let grid_valid = profile
        .and_then(|p| normalize_optional_string(p.grid.as_deref()))
        .is_some();
    fields.push(SetupFieldValidation {
        field: "grid".to_string(),
        valid: grid_valid,
        message: if grid_valid {
            String::new()
        } else {
            "Grid square is required.".to_string()
        },
    });

    let all_valid = callsign_valid && name_valid && operator_valid && grid_valid;
    ValidateSetupStepResponse {
        valid: all_valid,
        fields,
    }
}

fn validate_qrz_step(request: &ValidateSetupStepRequest) -> ValidateSetupStepResponse {
    let username = normalize_optional_string(request.qrz_xml_username.as_deref());
    let password = normalize_optional_string(request.qrz_xml_password.as_deref());
    let mut fields = Vec::new();

    // Both or neither must be set — absent is valid (skip QRZ).
    let paired = username.is_some() == password.is_some();

    fields.push(SetupFieldValidation {
        field: "qrz_xml_username".to_string(),
        valid: paired || username.is_some(),
        message: if !paired && username.is_none() {
            "Username is required when password is set.".to_string()
        } else {
            String::new()
        },
    });

    fields.push(SetupFieldValidation {
        field: "qrz_xml_password".to_string(),
        valid: paired || password.is_some(),
        message: if !paired && password.is_none() {
            "Password is required when username is set.".to_string()
        } else {
            String::new()
        },
    });

    ValidateSetupStepResponse {
        valid: paired,
        fields,
    }
}

async fn test_qrz_login(
    username: &str,
    password: &str,
    runtime_config: &RuntimeConfigManager,
) -> TestQrzCredentialsResponse {
    let username = username.trim();
    let password = password.trim();

    if username.is_empty() || password.is_empty() {
        return TestQrzCredentialsResponse {
            success: false,
            error_message: "Username and password are both required.".to_string(),
        };
    }

    // Build a temporary QRZ config using the test credentials plus the current
    // runtime user-agent setting (required by QRZ to identify clients).
    let effective = runtime_config.effective_values().await;
    let user_agent = effective
        .get(QRZ_USER_AGENT_ENV_VAR)
        .cloned()
        .unwrap_or_else(|| format!("QsoRipper/0.1.0 ({username})"));

    let config = QrzXmlConfig::from_value_provider(|name| match name {
        n if n == QRZ_XML_USERNAME_ENV_VAR => Some(username.to_string()),
        n if n == QRZ_XML_PASSWORD_ENV_VAR => Some(password.to_string()),
        n if n == QRZ_USER_AGENT_ENV_VAR => Some(user_agent.clone()),
        _ => effective.get(name).cloned(),
    });

    let config = match config {
        Ok(c) => c,
        Err(error) => {
            return TestQrzCredentialsResponse {
                success: false,
                error_message: format!("Invalid QRZ configuration: {error}"),
            };
        }
    };

    let provider = match QrzXmlProvider::new(config) {
        Ok(p) => p,
        Err(error) => {
            return TestQrzCredentialsResponse {
                success: false,
                error_message: format!("Failed to create QRZ provider: {error}"),
            };
        }
    };

    match provider.test_login().await {
        Ok(()) => TestQrzCredentialsResponse {
            success: true,
            error_message: String::new(),
        },
        Err(error) => TestQrzCredentialsResponse {
            success: false,
            error_message: format!("{error}"),
        },
    }
}

async fn test_qrz_logbook_api_key(
    api_key: &str,
    runtime_config: &RuntimeConfigManager,
) -> TestQrzLogbookCredentialsResponse {
    let api_key = api_key.trim();
    if api_key.is_empty() {
        return TestQrzLogbookCredentialsResponse {
            success: false,
            error_message: "API key is required.".to_string(),
            qso_count: None,
            logbook_owner: None,
        };
    }

    let effective = runtime_config.effective_values().await;
    let base_url = effective
        .get(QRZ_LOGBOOK_BASE_URL_ENV_VAR)
        .cloned()
        .unwrap_or_else(|| DEFAULT_QRZ_LOGBOOK_BASE_URL.to_string());

    let config = QrzLogbookConfig::new(api_key.to_string(), base_url, "QsoRipper/1.0".to_string());

    let client = match QrzLogbookClient::new(config) {
        Ok(c) => c,
        Err(error) => {
            return TestQrzLogbookCredentialsResponse {
                success: false,
                error_message: format!("Failed to create logbook client: {error}"),
                qso_count: None,
                logbook_owner: None,
            };
        }
    };

    match client.test_connection().await {
        Ok(status) => TestQrzLogbookCredentialsResponse {
            success: true,
            error_message: String::new(),
            qso_count: Some(status.qso_count),
            logbook_owner: Some(status.owner),
        },
        Err(error) => TestQrzLogbookCredentialsResponse {
            success: false,
            error_message: format!("{error}"),
            qso_count: None,
            logbook_owner: None,
        },
    }
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
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    deprecated
)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tonic::Request;

    use super::{
        build_log_file_step, build_qrz_integration_step, build_review_step,
        build_station_profiles_step, build_wizard_steps, default_config_path,
        suggested_log_file_path, validate_log_file_step, validate_qrz_step,
        validate_station_profiles_step, PersistedSetupConfig, SetupControlSurface, SetupState,
        StationProfileControlSurface, DEFAULT_CONFIG_FILE_NAME, RIGCTLD_ENABLED_ENV_VAR,
        RIGCTLD_HOST_ENV_VAR, RIGCTLD_PORT_ENV_VAR, RIGCTLD_READ_TIMEOUT_MS_ENV_VAR,
        RIGCTLD_STALE_THRESHOLD_MS_ENV_VAR,
    };
    use crate::runtime_config::RuntimeConfigManager;
    use qsoripper_core::proto::qsoripper::domain::{ConflictPolicy, StationProfile, SyncConfig};
    use qsoripper_core::proto::qsoripper::services::{
        setup_service_server::SetupService, station_profile_service_server::StationProfileService,
        GetActiveStationContextRequest, GetSetupStatusRequest, GetSetupWizardStateRequest,
        ListStationProfilesRequest, RigControlSettings, SaveSetupRequest,
        SaveStationProfileRequest, SetActiveStationProfileRequest,
        SetSessionStationProfileOverrideRequest, SetupWizardStep, StorageBackend,
        ValidateSetupStepRequest,
    };

    fn unique_config_path() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "qsoripper-setup-test-{}-{suffix}",
                std::process::id()
            ))
            .join(DEFAULT_CONFIG_FILE_NAME)
    }

    fn absolute_log_file_path(config_path: &std::path::Path, file_name: &str) -> String {
        config_path
            .parent()
            .expect("config directory")
            .join(file_name)
            .display()
            .to_string()
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
                .into_inner()
                .status
                .expect("status payload");

        assert!(!status.config_file_exists);
        assert!(!status.setup_complete);
        assert_eq!(config_path.display().to_string(), status.config_path);
        assert_eq!(
            suggested_log_file_path(&config_path).display().to_string(),
            status.suggested_log_file_path
        );
        assert_eq!(status.suggested_log_file_path, status.suggested_sqlite_path);
        assert!(status
            .warnings
            .contains(&"No persisted QsoRipper setup exists yet.".to_string()));
    }

    #[tokio::test]
    async fn get_setup_status_reads_legacy_sqlite_storage_as_log_file_path() {
        let config_path = unique_config_path();
        let config_directory = config_path.parent().expect("config directory");
        fs::create_dir_all(config_directory).expect("create config directory");
        fs::write(
            &config_path,
            r#"[storage]
backend = "sqlite"
sqlite_path = 'legacy\portable.db'

[station_profile]
station_callsign = "K7RND"
"#,
        )
        .expect("write legacy config");

        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let status =
            SetupService::get_setup_status(&service, Request::new(GetSetupStatusRequest {}))
                .await
                .expect("status")
                .into_inner()
                .status
                .expect("status payload");

        assert!(status.config_file_exists);
        assert!(status.setup_complete);
        assert_eq!(StorageBackend::Sqlite as i32, status.storage_backend);
        assert_eq!(Some("legacy\\portable.db"), status.log_file_path.as_deref());
        assert_eq!(status.log_file_path, status.sqlite_path);
        assert!(status.warnings.is_empty());

        fs::remove_dir_all(config_directory).expect("remove temp config directory");
    }

    #[tokio::test]
    async fn get_setup_status_flags_legacy_memory_setup_for_migration() {
        let config_path = unique_config_path();
        let config_directory = config_path.parent().expect("config directory");
        fs::create_dir_all(config_directory).expect("create config directory");
        fs::write(
            &config_path,
            r#"[storage]
backend = "memory"

[station_profile]
station_callsign = "K7RND"
"#,
        )
        .expect("write legacy config");

        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let status =
            SetupService::get_setup_status(&service, Request::new(GetSetupStatusRequest {}))
                .await
                .expect("status")
                .into_inner()
                .status
                .expect("status payload");

        assert!(status.config_file_exists);
        assert!(!status.setup_complete);
        assert_eq!(StorageBackend::Memory as i32, status.storage_backend);
        assert!(status.log_file_path.is_none());
        assert!(status
            .warnings
            .iter()
            .any(|warning| warning.contains("legacy in-memory storage")));

        fs::remove_dir_all(config_directory).expect("remove temp config directory");
    }

    #[tokio::test]
    async fn save_setup_persists_config_and_hot_applies_runtime_values() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "portable.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Unspecified as i32,
                sqlite_path: None,
                log_file_path: Some(log_file_path.clone()),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    operator_name: Some("Randy".to_string()),
                    arrl_section: Some("WWA".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: Some("k7rnd".to_string()),
                qrz_xml_password: Some("secret".to_string()),
                ..Default::default()
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
        assert_eq!(
            Some(log_file_path.as_str()),
            status.log_file_path.as_deref()
        );
        assert_eq!(status.log_file_path, status.sqlite_path);
        assert_eq!(Some("Home"), station_profile.profile_name.as_deref());
        assert_eq!("K7RND", station_profile.station_callsign);
        assert_eq!(Some("WWA"), station_profile.arrl_section.as_deref());
        assert!(config_path.exists());
        let saved_config = fs::read_to_string(&config_path).expect("saved config");
        let parsed_config =
            toml::from_str::<PersistedSetupConfig>(&saved_config).expect("parse saved config");
        assert_eq!(
            Some(log_file_path.as_str()),
            parsed_config.logbook.file_path.as_deref()
        );
        assert_eq!(
            Some("WWA"),
            parsed_config.station_profile.arrl_section.as_deref()
        );
        assert!(parsed_config.storage.backend.is_none());
        assert!(parsed_config.storage.sqlite_path.is_none());

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

    #[allow(clippy::too_many_lines)]
    #[tokio::test]
    async fn save_setup_preserves_existing_station_profiles() {
        let config_path = unique_config_path();
        let initial_log_file_path = absolute_log_file_path(&config_path, "home.db");
        let updated_log_file_path = absolute_log_file_path(&config_path, "updated.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let setup_service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());
        let station_profile_service =
            StationProfileControlSurface::new(setup_state.clone(), runtime_config.clone());

        SetupService::save_setup(
            &setup_service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Unspecified as i32,
                sqlite_path: None,
                log_file_path: Some(initial_log_file_path),
                station_profile: Some(StationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: "k7rnd".to_string(),
                    grid: Some("CN87".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: None,
                qrz_xml_password: None,
                ..Default::default()
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
                storage_backend: StorageBackend::Unspecified as i32,
                sqlite_path: None,
                log_file_path: Some(updated_log_file_path.clone()),
                station_profile: Some(StationProfile {
                    profile_name: Some("Home Debug".to_string()),
                    station_callsign: "k7rnd".to_string(),
                    grid: Some("CN86".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: Some("k7rnd".to_string()),
                qrz_xml_password: Some("secret".to_string()),
                ..Default::default()
            }),
        )
        .await
        .expect("updated setup")
        .into_inner()
        .status
        .expect("status");

        assert_eq!(StorageBackend::Sqlite as i32, updated.storage_backend);
        assert_eq!(
            Some(updated_log_file_path.as_str()),
            updated.log_file_path.as_deref()
        );
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

        drop(station_profile_service);
        drop(setup_service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        fs::remove_dir_all(config_directory).expect("remove temp config directory");
    }

    #[tokio::test]
    async fn save_setup_rejects_partial_qrz_credentials() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "partial.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let error = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Unspecified as i32,
                sqlite_path: None,
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                qrz_xml_username: Some("k7rnd".to_string()),
                qrz_xml_password: None,
                ..Default::default()
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
        let log_file_path = absolute_log_file_path(&config_path, "station-profiles.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let setup_service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());
        let station_profile_service =
            StationProfileControlSurface::new(setup_state.clone(), runtime_config.clone());

        SetupService::save_setup(
            &setup_service,
            Request::new(SaveSetupRequest {
                storage_backend: StorageBackend::Unspecified as i32,
                sqlite_path: None,
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: "k7rnd".to_string(),
                    grid: Some("CN87".to_string()),
                    ..StationProfile::default()
                }),
                qrz_xml_username: None,
                qrz_xml_password: None,
                ..Default::default()
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
        .into_inner()
        .context
        .expect("context payload");
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

        drop(station_profile_service);
        drop(setup_service);
        drop(runtime_config);
        drop(setup_state);

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

    // ── Wizard step builder tests ───────────────────────────────────────────

    #[test]
    fn build_wizard_steps_none_config_yields_four_steps() {
        let steps = build_wizard_steps(None);
        assert_eq!(4, steps.len());
        assert_eq!(i32::from(SetupWizardStep::LogFile), steps[0].step);
        assert_eq!(i32::from(SetupWizardStep::StationProfiles), steps[1].step);
        assert_eq!(i32::from(SetupWizardStep::QrzIntegration), steps[2].step);
        assert_eq!(i32::from(SetupWizardStep::Review), steps[3].step);
    }

    #[test]
    fn log_file_step_incomplete_when_no_config() {
        let step = build_log_file_step(None);
        assert!(!step.complete);
        assert!(!step.issues.is_empty());
    }

    #[test]
    fn log_file_step_complete_when_log_file_set() {
        let mut config = PersistedSetupConfig::default();
        config.logbook.file_path = Some("/tmp/test.db".to_string());
        let step = build_log_file_step(Some(&config));
        assert!(step.complete);
        assert!(step.issues.is_empty());
    }

    #[test]
    fn station_profiles_step_incomplete_when_no_profiles() {
        let step = build_station_profiles_step(None);
        assert!(!step.complete);
        assert!(step.issues.iter().any(|i| i.contains("At least one")));
    }

    #[test]
    fn station_profiles_step_incomplete_without_active_profile() {
        // The catalog falls back to the first entry when no explicit active ID is set,
        // so we must also verify that having no entries at all is incomplete.
        let config = PersistedSetupConfig::default();
        let step = build_station_profiles_step(Some(&config));
        assert!(!step.complete);
        assert!(step.issues.iter().any(|i| i.contains("At least one")));
    }

    #[test]
    fn station_profiles_step_complete_with_active_profile() {
        let mut config = PersistedSetupConfig::default();
        config
            .station_profiles
            .entries
            .push(super::PersistedStationProfileEntry {
                profile_id: "home".to_string(),
                profile: super::PersistedStationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: Some("K7RND".to_string()),
                    ..Default::default()
                },
            });
        config.station_profiles.active_profile_id = Some("home".to_string());
        let step = build_station_profiles_step(Some(&config));
        assert!(step.complete);
        assert!(step.issues.is_empty());
    }

    #[test]
    fn qrz_step_complete_when_both_absent() {
        let step = build_qrz_integration_step(None);
        assert!(step.complete);
        assert!(step.issues.is_empty());
    }

    #[test]
    fn qrz_step_complete_when_both_present() {
        let mut config = PersistedSetupConfig::default();
        config.qrz_xml.username = Some("user".to_string());
        config.qrz_xml.password = Some("pass".to_string());
        let step = build_qrz_integration_step(Some(&config));
        assert!(step.complete);
        assert!(step.issues.is_empty());
    }

    #[test]
    fn qrz_step_incomplete_when_only_username() {
        let mut config = PersistedSetupConfig::default();
        config.qrz_xml.username = Some("user".to_string());
        let step = build_qrz_integration_step(Some(&config));
        assert!(!step.complete);
        assert!(!step.issues.is_empty());
    }

    #[test]
    fn review_step_incomplete_when_prior_steps_incomplete() {
        let step = build_review_step(None);
        assert!(!step.complete);
    }

    #[test]
    fn review_step_complete_when_all_prior_complete() {
        let mut config = PersistedSetupConfig::default();
        config.logbook.file_path = Some("/tmp/test.db".to_string());
        config
            .station_profiles
            .entries
            .push(super::PersistedStationProfileEntry {
                profile_id: "home".to_string(),
                profile: super::PersistedStationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: Some("K7RND".to_string()),
                    ..Default::default()
                },
            });
        config.station_profiles.active_profile_id = Some("home".to_string());
        let step = build_review_step(Some(&config));
        assert!(step.complete);
        assert!(step.issues.is_empty());
    }

    // ── Validation tests ────────────────────────────────────────────────────

    #[test]
    fn validate_log_file_step_rejects_empty_path() {
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::LogFile.into(),
            log_file_path: None,
            ..Default::default()
        };
        let result = validate_log_file_step(&request);
        assert!(!result.valid);
        assert_eq!(1, result.fields.len());
        assert!(!result.fields[0].valid);
    }

    #[test]
    fn validate_log_file_step_accepts_valid_path() {
        let dir = std::env::temp_dir();
        let path = dir.join("test-validate.db");
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::LogFile.into(),
            log_file_path: Some(path.display().to_string()),
            ..Default::default()
        };
        let result = validate_log_file_step(&request);
        assert!(result.valid);
        assert!(result.fields[0].valid);
    }

    #[test]
    fn validate_station_profiles_rejects_empty_profile() {
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::StationProfiles.into(),
            station_profile: Some(StationProfile::default()),
            ..Default::default()
        };
        let result = validate_station_profiles_step(&request);
        assert!(!result.valid);
        assert_eq!(4, result.fields.len());
    }

    #[test]
    fn validate_station_profiles_accepts_complete_profile() {
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::StationProfiles.into(),
            station_profile: Some(StationProfile {
                profile_name: Some("Home".to_string()),
                station_callsign: "K7RND".to_string(),
                operator_callsign: Some("K7RND".to_string()),
                grid: Some("CN87".to_string()),
                ..Default::default()
            }),
            ..Default::default()
        };
        let result = validate_station_profiles_step(&request);
        assert!(result.valid);
        assert!(result.fields.iter().all(|f| f.valid));
    }

    #[test]
    fn validate_qrz_step_accepts_both_absent() {
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::QrzIntegration.into(),
            ..Default::default()
        };
        let result = validate_qrz_step(&request);
        assert!(result.valid);
    }

    #[test]
    fn validate_qrz_step_accepts_both_present() {
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::QrzIntegration.into(),
            qrz_xml_username: Some("user".to_string()),
            qrz_xml_password: Some("pass".to_string()),
            ..Default::default()
        };
        let result = validate_qrz_step(&request);
        assert!(result.valid);
    }

    #[test]
    fn validate_qrz_step_rejects_partial_credentials() {
        let request = ValidateSetupStepRequest {
            step: SetupWizardStep::QrzIntegration.into(),
            qrz_xml_username: Some("user".to_string()),
            qrz_xml_password: None,
            ..Default::default()
        };
        let result = validate_qrz_step(&request);
        assert!(!result.valid);
    }

    // ── is_first_run tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn is_first_run_true_when_no_config() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let status =
            SetupService::get_setup_status(&service, Request::new(GetSetupStatusRequest {}))
                .await
                .expect("status")
                .into_inner()
                .status
                .expect("status payload");

        assert!(status.is_first_run);
    }

    #[tokio::test]
    async fn is_first_run_false_after_save() {
        let config_path = unique_config_path();
        let config_directory = config_path.parent().expect("config directory");
        fs::create_dir_all(config_directory).expect("create config directory");

        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let log_file = absolute_log_file_path(&config_path, "test.db");
        SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file),
                station_profile: Some(StationProfile {
                    profile_name: Some("Home".to_string()),
                    station_callsign: "K7RND".to_string(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("save setup");

        let status =
            SetupService::get_setup_status(&service, Request::new(GetSetupStatusRequest {}))
                .await
                .expect("status")
                .into_inner()
                .status
                .expect("status payload");

        assert!(!status.is_first_run);

        drop(service);
        let _ = fs::remove_dir_all(config_directory);
    }

    // ── GetSetupWizardState RPC test ────────────────────────────────────────

    #[tokio::test]
    async fn get_wizard_state_returns_steps_and_status() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let response = SetupService::get_setup_wizard_state(
            &service,
            Request::new(GetSetupWizardStateRequest {}),
        )
        .await
        .expect("wizard state")
        .into_inner();

        assert!(response.status.is_some());
        assert_eq!(4, response.steps.len());
        assert!(response.station_profiles.is_empty());

        // For a fresh config, LogFile should be incomplete
        let log_step = &response.steps[0];
        assert_eq!(i32::from(SetupWizardStep::LogFile), log_step.step);
        assert!(!log_step.complete);
    }

    // ── ValidateSetupStep RPC test ──────────────────────────────────────────

    #[tokio::test]
    async fn validate_step_rpc_validates_log_file() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let dir = std::env::temp_dir();
        let path = dir.join("validate-rpc-test.db");
        let response = SetupService::validate_setup_step(
            &service,
            Request::new(ValidateSetupStepRequest {
                step: SetupWizardStep::LogFile.into(),
                log_file_path: Some(path.display().to_string()),
                ..Default::default()
            }),
        )
        .await
        .expect("validate")
        .into_inner();

        assert!(response.valid);
    }

    // ── TestQrzCredentials RPC test (empty creds) ───────────────────────────

    #[tokio::test]
    async fn test_qrz_credentials_rejects_empty() {
        let config_path = unique_config_path();
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let response = SetupService::test_qrz_credentials(
            &service,
            Request::new(
                qsoripper_core::proto::qsoripper::services::TestQrzCredentialsRequest {
                    qrz_xml_username: String::new(),
                    qrz_xml_password: String::new(),
                },
            ),
        )
        .await
        .expect("test qrz")
        .into_inner();

        assert!(!response.success);
        assert!(!response.error_message.is_empty());
    }

    // ── QRZ logbook API key and sync config tests ───────────────────────────

    #[tokio::test]
    async fn save_setup_persists_logbook_api_key_and_reports_in_status() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "logbook-key.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        // Save with logbook API key
        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                qrz_logbook_api_key: Some("abc-123-logbook-key".to_string()),
                ..Default::default()
            }),
        )
        .await
        .expect("save setup")
        .into_inner();

        let status = response.status.expect("status payload");
        assert!(status.has_qrz_logbook_api_key);

        // Verify it round-trips through persisted config on disk
        let saved_toml = fs::read_to_string(&config_path).expect("read config");
        let parsed =
            toml::from_str::<PersistedSetupConfig>(&saved_toml).expect("parse saved config");
        assert_eq!(
            Some("abc-123-logbook-key"),
            parsed.qrz_logbook.api_key.as_deref()
        );

        // Verify runtime values include the key
        let runtime_values = setup_state.runtime_config_values().await;
        assert_eq!(
            Some("abc-123-logbook-key"),
            runtime_values
                .get(crate::runtime_config::QRZ_LOGBOOK_API_KEY_ENV_VAR)
                .map(String::as_str)
        );

        drop(service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        let _ = fs::remove_dir_all(config_directory);
    }

    #[tokio::test]
    async fn save_setup_without_logbook_key_reports_false() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "no-logbook-key.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state, runtime_config);

        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("save setup")
        .into_inner();

        let status = response.status.expect("status payload");
        assert!(!status.has_qrz_logbook_api_key);
        // Default sync config should be present
        let sync = status.sync_config.expect("sync_config should be present");
        assert!(!sync.auto_sync_enabled);
        assert_eq!(300, sync.sync_interval_seconds);
        assert_eq!(ConflictPolicy::LastWriteWins as i32, sync.conflict_policy);

        drop(service);
        let config_directory = config_path.parent().expect("config directory");
        let _ = fs::remove_dir_all(config_directory);
    }

    #[tokio::test]
    async fn save_setup_persists_sync_config_and_round_trips() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "sync-config.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                sync_config: Some(SyncConfig {
                    auto_sync_enabled: true,
                    sync_interval_seconds: 600,
                    conflict_policy: ConflictPolicy::FlagForReview as i32,
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("save setup")
        .into_inner();

        let status = response.status.expect("status payload");
        let sync = status.sync_config.expect("sync_config");
        assert!(sync.auto_sync_enabled);
        assert_eq!(600, sync.sync_interval_seconds);
        assert_eq!(ConflictPolicy::FlagForReview as i32, sync.conflict_policy);

        // Verify persisted TOML
        let saved_toml = fs::read_to_string(&config_path).expect("read config");
        let parsed =
            toml::from_str::<PersistedSetupConfig>(&saved_toml).expect("parse saved config");
        assert!(parsed.sync.auto_sync_enabled);
        assert_eq!(600, parsed.sync.sync_interval_seconds);
        assert_eq!("flag_for_review", parsed.sync.conflict_policy);

        // Verify runtime values
        let runtime_values = setup_state.runtime_config_values().await;
        assert_eq!(
            Some("true"),
            runtime_values
                .get(crate::runtime_config::SYNC_AUTO_ENABLED_ENV_VAR)
                .map(String::as_str)
        );
        assert_eq!(
            Some("600"),
            runtime_values
                .get(crate::runtime_config::SYNC_INTERVAL_SECONDS_ENV_VAR)
                .map(String::as_str)
        );
        assert_eq!(
            Some("flag_for_review"),
            runtime_values
                .get(crate::runtime_config::SYNC_CONFLICT_POLICY_ENV_VAR)
                .map(String::as_str)
        );

        drop(service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        let _ = fs::remove_dir_all(config_directory);
    }

    #[tokio::test]
    async fn save_setup_persists_rig_control_and_round_trips() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "rig-control.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                rig_control: Some(RigControlSettings {
                    enabled: Some(true),
                    host: Some("127.0.0.1".to_string()),
                    port: Some(4532),
                    read_timeout_ms: Some(2500),
                    stale_threshold_ms: Some(6000),
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("save setup")
        .into_inner();

        let status = response.status.expect("status payload");
        let rig_control = status.rig_control.expect("rig_control");
        assert_eq!(Some(true), rig_control.enabled);
        assert_eq!(Some("127.0.0.1"), rig_control.host.as_deref());
        assert_eq!(Some(4532), rig_control.port);
        assert_eq!(Some(2500), rig_control.read_timeout_ms);
        assert_eq!(Some(6000), rig_control.stale_threshold_ms);

        let saved_toml = fs::read_to_string(&config_path).expect("read config");
        let parsed =
            toml::from_str::<PersistedSetupConfig>(&saved_toml).expect("parse saved config");
        assert_eq!(Some(true), parsed.rig_control.enabled);
        assert_eq!(Some("127.0.0.1"), parsed.rig_control.host.as_deref());
        assert_eq!(Some(4532), parsed.rig_control.port);
        assert_eq!(Some(2500), parsed.rig_control.read_timeout_ms);
        assert_eq!(Some(6000), parsed.rig_control.stale_threshold_ms);

        let runtime_values = setup_state.runtime_config_values().await;
        assert_eq!(
            Some("true"),
            runtime_values
                .get(RIGCTLD_ENABLED_ENV_VAR)
                .map(String::as_str)
        );
        assert_eq!(
            Some("127.0.0.1"),
            runtime_values.get(RIGCTLD_HOST_ENV_VAR).map(String::as_str)
        );
        assert_eq!(
            Some("4532"),
            runtime_values.get(RIGCTLD_PORT_ENV_VAR).map(String::as_str)
        );
        assert_eq!(
            Some("2500"),
            runtime_values
                .get(RIGCTLD_READ_TIMEOUT_MS_ENV_VAR)
                .map(String::as_str)
        );
        assert_eq!(
            Some("6000"),
            runtime_values
                .get(RIGCTLD_STALE_THRESHOLD_MS_ENV_VAR)
                .map(String::as_str)
        );

        drop(service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        let _ = fs::remove_dir_all(config_directory);
    }

    #[tokio::test]
    async fn save_setup_preserves_rig_control_when_omitted_in_subsequent_save() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "preserve-rig-control.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path.clone()),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                rig_control: Some(RigControlSettings {
                    enabled: Some(true),
                    host: Some("127.0.0.1".to_string()),
                    port: Some(4532),
                    ..Default::default()
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("first save");

        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                ..Default::default()
            }),
        )
        .await
        .expect("second save")
        .into_inner();

        let rig_control = response
            .status
            .expect("status payload")
            .rig_control
            .expect("rig_control");
        assert_eq!(Some(true), rig_control.enabled);
        assert_eq!(Some("127.0.0.1"), rig_control.host.as_deref());
        assert_eq!(Some(4532), rig_control.port);

        drop(service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        let _ = fs::remove_dir_all(config_directory);
    }

    #[tokio::test]
    async fn save_setup_preserves_logbook_key_when_omitted_in_subsequent_save() {
        let config_path = unique_config_path();
        let log_file_path = absolute_log_file_path(&config_path, "preserve-key.db");
        let setup_state = Arc::new(SetupState::load(config_path.clone()).expect("setup state"));
        let runtime_config = Arc::new(RuntimeConfigManager::new(BTreeMap::new()).expect("runtime"));
        let service = SetupControlSurface::new(setup_state.clone(), runtime_config.clone());

        // First save: set the key
        SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path.clone()),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                qrz_logbook_api_key: Some("original-key".to_string()),
                ..Default::default()
            }),
        )
        .await
        .expect("first save");

        // Second save: omit the key
        let response = SetupService::save_setup(
            &service,
            Request::new(SaveSetupRequest {
                log_file_path: Some(log_file_path),
                station_profile: Some(StationProfile {
                    station_callsign: "k7rnd".to_string(),
                    ..StationProfile::default()
                }),
                // qrz_logbook_api_key omitted
                ..Default::default()
            }),
        )
        .await
        .expect("second save")
        .into_inner();

        let status = response.status.expect("status payload");
        assert!(
            status.has_qrz_logbook_api_key,
            "logbook key should be preserved across saves when omitted"
        );

        drop(service);
        drop(runtime_config);
        drop(setup_state);

        let config_directory = config_path.parent().expect("config directory");
        let _ = fs::remove_dir_all(config_directory);
    }

    #[test]
    fn persisted_sync_config_round_trips_through_proto() {
        let proto = SyncConfig {
            auto_sync_enabled: true,
            sync_interval_seconds: 120,
            conflict_policy: ConflictPolicy::FlagForReview as i32,
        };
        let persisted = super::PersistedSyncConfig::from_proto(&proto);
        assert!(persisted.auto_sync_enabled);
        assert_eq!(120, persisted.sync_interval_seconds);
        assert_eq!("flag_for_review", persisted.conflict_policy);

        let back = persisted.to_proto();
        assert!(back.auto_sync_enabled);
        assert_eq!(120, back.sync_interval_seconds);
        assert_eq!(ConflictPolicy::FlagForReview as i32, back.conflict_policy);
    }

    #[test]
    fn persisted_sync_config_defaults_interval_when_zero() {
        let proto = SyncConfig {
            auto_sync_enabled: false,
            sync_interval_seconds: 0,
            conflict_policy: ConflictPolicy::LastWriteWins as i32,
        };
        let persisted = super::PersistedSyncConfig::from_proto(&proto);
        assert_eq!(300, persisted.sync_interval_seconds);

        let back = persisted.to_proto();
        assert_eq!(300, back.sync_interval_seconds);
    }

    #[test]
    fn persisted_rig_control_config_round_trips_through_proto() {
        let proto = RigControlSettings {
            enabled: Some(true),
            host: Some("127.0.0.1".to_string()),
            port: Some(4532),
            read_timeout_ms: Some(2000),
            stale_threshold_ms: Some(5000),
        };
        let persisted =
            super::PersistedRigControlConfig::from_proto(&proto).expect("rig control config");
        assert_eq!(Some(true), persisted.enabled);
        assert_eq!(Some("127.0.0.1"), persisted.host.as_deref());
        assert_eq!(Some(4532), persisted.port);
        assert_eq!(Some(2000), persisted.read_timeout_ms);
        assert_eq!(Some(5000), persisted.stale_threshold_ms);

        let back = persisted.to_proto().expect("rig control proto");
        assert_eq!(proto.enabled, back.enabled);
        assert_eq!(proto.host, back.host);
        assert_eq!(proto.port, back.port);
        assert_eq!(proto.read_timeout_ms, back.read_timeout_ms);
        assert_eq!(proto.stale_threshold_ms, back.stale_threshold_ms);
    }

    #[test]
    fn persisted_rig_control_config_rejects_invalid_port() {
        let error = super::PersistedRigControlConfig::from_proto(&RigControlSettings {
            port: Some(u32::from(u16::MAX) + 1),
            ..Default::default()
        })
        .expect_err("invalid port should be rejected");
        assert_eq!("Rig control port must be between 1 and 65535.", error);
    }
}
