//! Provider seam for callsign lookup data sources.

use crate::proto::logripper::domain::{CallsignRecord, DebugHttpExchange};

/// Abstraction over an external callsign data provider.
#[tonic::async_trait]
pub trait CallsignProvider: Send + Sync {
    /// Look up a normalized callsign and return a provider-normalized result.
    async fn lookup_callsign(&self, callsign: &str) -> Result<ProviderLookup, ProviderLookupError>;
}

/// Provider lookup outcomes.
#[derive(Debug, Clone)]
pub struct ProviderLookup {
    /// The provider-normalized lookup outcome.
    pub outcome: ProviderLookupOutcome,
    /// Redacted provider request/response exchanges captured during the lookup.
    pub debug_http_exchanges: Vec<DebugHttpExchange>,
}

impl ProviderLookup {
    /// Build a found result with any captured provider exchanges.
    #[must_use]
    pub fn found(record: CallsignRecord, debug_http_exchanges: Vec<DebugHttpExchange>) -> Self {
        Self {
            outcome: ProviderLookupOutcome::Found(Box::new(record)),
            debug_http_exchanges,
        }
    }

    /// Build a not-found result with any captured provider exchanges.
    #[must_use]
    pub fn not_found(debug_http_exchanges: Vec<DebugHttpExchange>) -> Self {
        Self {
            outcome: ProviderLookupOutcome::NotFound,
            debug_http_exchanges,
        }
    }
}

/// Provider lookup outcomes without transport/error state.
#[derive(Debug, Clone)]
pub enum ProviderLookupOutcome {
    /// The callsign exists and has a normalized record.
    Found(Box<CallsignRecord>),
    /// The provider confirms the callsign does not exist.
    NotFound,
}

/// Stable provider error categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderLookupErrorKind {
    /// Provider configuration is missing or invalid.
    Configuration,
    /// Provider authentication failed.
    Authentication,
    /// Provider session state is invalid or expired.
    Session,
    /// Provider transport failed before a valid response was received.
    Transport,
    /// Provider returned a payload that could not be parsed or understood.
    Parse,
    /// Provider rejected the request due to throttling or rate limits.
    RateLimited,
}

/// Provider errors surfaced to lookup orchestration logic.
#[derive(Debug, Clone)]
pub struct ProviderLookupError {
    kind: ProviderLookupErrorKind,
    message: String,
    debug_http_exchanges: Vec<DebugHttpExchange>,
}

impl ProviderLookupError {
    /// Provider is unavailable due to configuration.
    #[must_use]
    pub fn configuration(message: impl Into<String>) -> Self {
        Self::new(ProviderLookupErrorKind::Configuration, message, Vec::new())
    }

    /// Provider authentication failed.
    #[must_use]
    pub fn authentication(
        message: impl Into<String>,
        debug_http_exchanges: Vec<DebugHttpExchange>,
    ) -> Self {
        Self::new(
            ProviderLookupErrorKind::Authentication,
            message,
            debug_http_exchanges,
        )
    }

    /// Provider session state is invalid.
    #[must_use]
    pub fn session(
        message: impl Into<String>,
        debug_http_exchanges: Vec<DebugHttpExchange>,
    ) -> Self {
        Self::new(
            ProviderLookupErrorKind::Session,
            message,
            debug_http_exchanges,
        )
    }

    /// Provider transport failed.
    #[must_use]
    pub fn transport(
        message: impl Into<String>,
        debug_http_exchanges: Vec<DebugHttpExchange>,
    ) -> Self {
        Self::new(
            ProviderLookupErrorKind::Transport,
            message,
            debug_http_exchanges,
        )
    }

    /// Provider returned an unexpected payload.
    #[must_use]
    pub fn parse(message: impl Into<String>, debug_http_exchanges: Vec<DebugHttpExchange>) -> Self {
        Self::new(
            ProviderLookupErrorKind::Parse,
            message,
            debug_http_exchanges,
        )
    }

    /// Provider indicated request throttling.
    #[must_use]
    pub fn rate_limited(
        message: impl Into<String>,
        debug_http_exchanges: Vec<DebugHttpExchange>,
    ) -> Self {
        Self::new(
            ProviderLookupErrorKind::RateLimited,
            message,
            debug_http_exchanges,
        )
    }

    fn new(
        kind: ProviderLookupErrorKind,
        message: impl Into<String>,
        debug_http_exchanges: Vec<DebugHttpExchange>,
    ) -> Self {
        Self {
            kind,
            message: message.into(),
            debug_http_exchanges,
        }
    }

    /// Returns the captured provider request/response exchanges associated with this failure.
    #[must_use]
    pub fn debug_http_exchanges(&self) -> &[DebugHttpExchange] {
        &self.debug_http_exchanges
    }

    /// Prepends earlier exchanges captured before this failure occurred.
    #[must_use]
    pub fn with_prior_debug_http_exchanges(mut self, mut prior: Vec<DebugHttpExchange>) -> Self {
        prior.extend(self.debug_http_exchanges);
        self.debug_http_exchanges = prior;
        self
    }

    /// Returns whether the error class is suitable for retry handling.
    #[must_use]
    pub fn is_retryable(&self) -> bool {
        matches!(
            self.kind,
            ProviderLookupErrorKind::Transport | ProviderLookupErrorKind::RateLimited
        )
    }
}

impl std::fmt::Display for ProviderLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Provider {} error: {}",
            match self.kind {
                ProviderLookupErrorKind::Configuration => "configuration",
                ProviderLookupErrorKind::Authentication => "authentication",
                ProviderLookupErrorKind::Session => "session",
                ProviderLookupErrorKind::Transport => "transport",
                ProviderLookupErrorKind::Parse => "parse",
                ProviderLookupErrorKind::RateLimited => "rate-limit",
            },
            self.message
        )
    }
}

impl std::error::Error for ProviderLookupError {}

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
        Err(ProviderLookupError::configuration(self.reason.clone()))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn retryable_errors_are_transport_and_rate_limit_only() {
        assert!(ProviderLookupError::transport("offline", Vec::new()).is_retryable());
        assert!(ProviderLookupError::rate_limited("slow down", Vec::new()).is_retryable());
        assert!(!ProviderLookupError::authentication("bad password", Vec::new()).is_retryable());
        assert!(!ProviderLookupError::configuration("missing").is_retryable());
        assert!(!ProviderLookupError::session("expired", Vec::new()).is_retryable());
        assert!(!ProviderLookupError::parse("invalid xml", Vec::new()).is_retryable());
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
