//! Rig control providers, normalization, and cached rig state snapshots.

pub mod band_mapping;
pub mod mode_mapping;
mod monitor;
mod provider;
pub mod rigctld;

pub use monitor::RigControlMonitor;
pub use provider::{
    DisabledRigControlProvider, RigControlProvider, RigControlProviderError,
    RigControlProviderErrorKind,
};
pub use rigctld::{
    RigctldConfig, RigctldProvider, DEFAULT_RIGCTLD_HOST, DEFAULT_RIGCTLD_PORT,
    DEFAULT_RIGCTLD_READ_TIMEOUT_MS, DEFAULT_RIGCTLD_STALE_THRESHOLD_MS, RIGCTLD_ENABLED_ENV_VAR,
    RIGCTLD_HOST_ENV_VAR, RIGCTLD_PORT_ENV_VAR, RIGCTLD_READ_TIMEOUT_MS_ENV_VAR,
    RIGCTLD_STALE_THRESHOLD_MS_ENV_VAR,
};
