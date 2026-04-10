//! Provider seam for callsign lookup data sources.

use crate::proto::logripper::domain::CallsignRecord;

/// Abstraction over an external callsign data provider.
#[tonic::async_trait]
pub trait CallsignProvider: Send + Sync {
    /// Look up a normalized callsign and return a provider-normalized result.
    async fn lookup_callsign(&self, callsign: &str) -> Result<ProviderLookup, ProviderLookupError>;
}

/// Provider lookup outcomes.
#[derive(Debug, Clone)]
pub enum ProviderLookup {
    /// The callsign exists and has a normalized record.
    Found(Box<CallsignRecord>),
    /// The provider confirms the callsign does not exist.
    NotFound,
}

/// Provider errors surfaced to lookup orchestration logic.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ProviderLookupError {
    /// Provider is unavailable due to configuration.
    #[error("Provider configuration error: {0}")]
    Configuration(String),
    /// Provider authentication failed.
    #[error("Provider authentication error: {0}")]
    Authentication(String),
    /// Provider session state is invalid.
    #[error("Provider session error: {0}")]
    Session(String),
    /// Provider transport failed.
    #[error("Provider transport error: {0}")]
    Transport(String),
    /// Provider returned an unexpected payload.
    #[error("Provider parse error: {0}")]
    Parse(String),
    /// Provider indicated request throttling.
    #[error("Provider rate-limit error: {0}")]
    RateLimited(String),
}

impl ProviderLookupError {
    /// Returns whether the error class is suitable for retry handling.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Transport(_) | Self::RateLimited(_))
    }
}

/// Provider used when configuration is missing or invalid.
#[derive(Debug, Clone)]
pub struct DisabledCallsignProvider {
    reason: String,
}

impl DisabledCallsignProvider {
    /// Create a disabled provider with a stable reason message.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

#[tonic::async_trait]
impl CallsignProvider for DisabledCallsignProvider {
    async fn lookup_callsign(
        &self,
        _callsign: &str,
    ) -> Result<ProviderLookup, ProviderLookupError> {
        Err(ProviderLookupError::Configuration(self.reason.clone()))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn retryable_errors_are_transport_and_rate_limit_only() {
        assert!(ProviderLookupError::Transport("offline".to_string()).is_retryable());
        assert!(ProviderLookupError::RateLimited("slow down".to_string()).is_retryable());
        assert!(!ProviderLookupError::Authentication("bad password".to_string()).is_retryable());
        assert!(!ProviderLookupError::Configuration("missing".to_string()).is_retryable());
        assert!(!ProviderLookupError::Session("expired".to_string()).is_retryable());
        assert!(!ProviderLookupError::Parse("invalid xml".to_string()).is_retryable());
    }

    #[tokio::test]
    async fn disabled_provider_returns_configuration_error() {
        let provider = DisabledCallsignProvider::new("missing config");

        let error = provider.lookup_callsign("W1AW").await.expect_err("error");

        assert_eq!(
            "Provider configuration error: missing config",
            error.to_string()
        );
    }
}
