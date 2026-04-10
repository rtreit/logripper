//! Callsign lookup orchestration and provider adapters.

pub mod coordinator;
pub mod provider;
pub mod qrz_xml;

pub use coordinator::{LookupCoordinator, LookupCoordinatorConfig};
pub use provider::{
    CallsignProvider, DisabledCallsignProvider, ProviderLookup, ProviderLookupError,
};
pub use qrz_xml::{QrzXmlConfig, QrzXmlConfigError, QrzXmlProvider};
