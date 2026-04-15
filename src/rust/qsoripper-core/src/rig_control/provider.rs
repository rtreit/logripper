//! Provider seam for rig control data.

use crate::proto::qsoripper::domain::RigSnapshot;

/// Abstraction over an external rig control provider (e.g., rigctld).
#[tonic::async_trait]
pub trait RigControlProvider: Send + Sync {
    /// Read the current rig state and return a normalized snapshot.
    async fn get_snapshot(&self) -> Result<RigSnapshot, RigControlProviderError>;
}

/// Stable provider error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RigControlProviderErrorKind {
    /// Provider is disabled by configuration.
    Disabled,
    /// Transport failed before a valid response was received.
    Transport,
    /// Provider returned a payload that could not be parsed.
    Parse,
    /// Read timed out waiting for a response.
    Timeout,
}

/// Errors surfaced by the rig control provider layer.
#[derive(Debug, Clone)]
pub struct RigControlProviderError {
    kind: RigControlProviderErrorKind,
    message: String,
}

impl RigControlProviderError {
    /// Provider is disabled.
    #[must_use]
    pub fn disabled(message: impl Into<String>) -> Self {
        Self::new(RigControlProviderErrorKind::Disabled, message)
    }

    /// Provider transport failed.
    #[must_use]
    pub fn transport(message: impl Into<String>) -> Self {
        Self::new(RigControlProviderErrorKind::Transport, message)
    }

    /// Provider returned an unexpected payload.
    #[must_use]
    pub fn parse(message: impl Into<String>) -> Self {
        Self::new(RigControlProviderErrorKind::Parse, message)
    }

    /// Read timed out.
    #[must_use]
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(RigControlProviderErrorKind::Timeout, message)
    }

    fn new(kind: RigControlProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Returns the error kind.
    #[must_use]
    pub fn kind(&self) -> RigControlProviderErrorKind {
        self.kind
    }

    /// Returns whether the error class is suitable for retry handling.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            RigControlProviderErrorKind::Transport | RigControlProviderErrorKind::Timeout
        )
    }
}

impl std::fmt::Display for RigControlProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Rig control provider {} error: {}",
            match self.kind {
                RigControlProviderErrorKind::Disabled => "disabled",
                RigControlProviderErrorKind::Transport => "transport",
                RigControlProviderErrorKind::Parse => "parse",
                RigControlProviderErrorKind::Timeout => "timeout",
            },
            self.message
        )
    }
}

impl std::error::Error for RigControlProviderError {}

/// Provider used when rig control is disabled by configuration.
#[derive(Debug, Clone)]
pub struct DisabledRigControlProvider {
    reason: String,
}

impl DisabledRigControlProvider {
    /// Create a disabled provider with a stable reason message.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[tonic::async_trait]
impl RigControlProvider for DisabledRigControlProvider {
    async fn get_snapshot(&self) -> Result<RigSnapshot, RigControlProviderError> {
        Err(RigControlProviderError::disabled(self.reason.clone()))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn retryable_errors_are_transport_and_timeout() {
        assert!(RigControlProviderError::transport("offline").is_retryable());
        assert!(RigControlProviderError::timeout("slow").is_retryable());
        assert!(!RigControlProviderError::disabled("off").is_retryable());
        assert!(!RigControlProviderError::parse("bad").is_retryable());
    }

    #[tokio::test]
    async fn disabled_provider_returns_disabled_error() {
        let provider = DisabledRigControlProvider::new("rig control not configured");

        let error = provider.get_snapshot().await.expect_err("error");

        assert_eq!(RigControlProviderErrorKind::Disabled, error.kind());
        assert_eq!(
            "Rig control provider disabled error: rig control not configured",
            error.to_string()
        );
    }
}
