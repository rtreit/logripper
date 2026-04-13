//! Callsign lookup orchestration and provider adapters.

pub mod coordinator;
pub mod provider;
pub mod qrz_xml;

pub use coordinator::{LookupCoordinator, LookupCoordinatorConfig};
pub use provider::{
    CallsignProvider, DisabledCallsignProvider, ProviderLookup, ProviderLookupError,
};
pub use qrz_xml::{
    QrzXmlConfig, QrzXmlConfigError, QrzXmlProvider, DEFAULT_QRZ_HTTP_TIMEOUT_SECONDS,
    DEFAULT_QRZ_MAX_RETRIES, DEFAULT_QRZ_XML_BASE_URL, QRZ_HTTP_TIMEOUT_SECONDS_ENV_VAR,
    QRZ_MAX_RETRIES_ENV_VAR, QRZ_USER_AGENT_ENV_VAR, QRZ_XML_BASE_URL_ENV_VAR,
    QRZ_XML_CAPTURE_ONLY_ENV_VAR, QRZ_XML_PASSWORD_ENV_VAR, QRZ_XML_USERNAME_ENV_VAR,
};
