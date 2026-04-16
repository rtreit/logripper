//! QRZ Logbook API client for bidirectional QSO sync.
//!
//! This module talks to the QRZ Logbook HTTP API (`https://logbook.qrz.com/api`)
//! to fetch, upload, and delete QSO records. Authentication uses a
//! per-user API key (distinct from the QRZ XML session key used for callsign
//! enrichment in [`crate::lookup::qrz_xml`]).
//!
//! Responses from the QRZ Logbook API come in two forms:
//! - **Key-value** pairs (`KEY=VALUE&KEY=VALUE`) for status, insert, and delete.
//! - **ADIF** payload for the FETCH action.
//!
//! The client reuses the existing [`crate::adif`] module for ADIF
//! parsing and serialization.

use std::{collections::HashMap, env, fmt, time::Duration};

use reqwest::Client;

use crate::{
    adif::{self, mapper::AdifMapper},
    proto::qsoripper::domain::QsoRecord,
};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default QRZ Logbook API endpoint.
pub const DEFAULT_QRZ_LOGBOOK_BASE_URL: &str = "https://logbook.qrz.com/api";

/// Default user-agent string sent with requests.
const DEFAULT_USER_AGENT: &str = "QsoRipper/0.1";

/// Environment variable that provides the QRZ Logbook API key.
pub const QRZ_LOGBOOK_API_KEY_ENV_VAR: &str = "QSORIPPER_QRZ_LOGBOOK_API_KEY";

/// Environment variable that overrides the QRZ Logbook base URL.
pub const QRZ_LOGBOOK_BASE_URL_ENV_VAR: &str = "QSORIPPER_QRZ_LOGBOOK_BASE_URL";

/// Environment variable that overrides the user-agent string.
pub const QRZ_LOGBOOK_USER_AGENT_ENV_VAR: &str = "QSORIPPER_QRZ_LOGBOOK_USER_AGENT";

/// Default HTTP timeout in seconds for logbook requests.
const DEFAULT_HTTP_TIMEOUT_SECONDS: u64 = 30;

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Configuration for the QRZ Logbook API client.
#[derive(Clone)]
pub struct QrzLogbookConfig {
    /// QRZ Logbook API key.
    api_key: String,
    /// Base URL for the logbook API endpoint.
    base_url: String,
    /// User-agent header sent with every request.
    user_agent: String,
    /// HTTP timeout per request.
    http_timeout: Duration,
}

impl fmt::Debug for QrzLogbookConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("QrzLogbookConfig")
            .field("api_key", &"<redacted>")
            .field("base_url", &self.base_url)
            .field("user_agent", &self.user_agent)
            .field("http_timeout", &self.http_timeout)
            .finish()
    }
}

impl QrzLogbookConfig {
    /// Create a configuration with explicit values.
    #[must_use]
    pub fn new(api_key: String, base_url: String, user_agent: String) -> Self {
        Self {
            api_key,
            base_url,
            user_agent,
            http_timeout: Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECONDS),
        }
    }

    /// Load configuration from environment variables.
    ///
    /// Required: `QSORIPPER_QRZ_LOGBOOK_API_KEY`
    /// Optional: `QSORIPPER_QRZ_LOGBOOK_BASE_URL`, `QSORIPPER_QRZ_LOGBOOK_USER_AGENT`
    ///
    /// # Errors
    ///
    /// Returns [`QrzLogbookError::AuthenticationFailed`] when the API key
    /// environment variable is missing or blank.
    pub fn from_env() -> Result<Self, QrzLogbookError> {
        let api_key = env::var(QRZ_LOGBOOK_API_KEY_ENV_VAR)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                QrzLogbookError::AuthenticationFailed(format!(
                    "Required environment variable '{QRZ_LOGBOOK_API_KEY_ENV_VAR}' is missing or blank"
                ))
            })?;

        let base_url = env::var(QRZ_LOGBOOK_BASE_URL_ENV_VAR)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_QRZ_LOGBOOK_BASE_URL.to_string());

        let user_agent = env::var(QRZ_LOGBOOK_USER_AGENT_ENV_VAR)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_USER_AGENT.to_string());

        Ok(Self::new(api_key, base_url, user_agent))
    }

    /// Return the configured base URL for diagnostics.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors produced by the QRZ Logbook client.
#[derive(Debug, thiserror::Error)]
pub enum QrzLogbookError {
    /// The API key was rejected or is missing.
    #[error("QRZ Logbook authentication failed: {0}")]
    AuthenticationFailed(String),

    /// QRZ Logbook returned `RESULT=FAIL` with a reason.
    #[error("QRZ Logbook API error: {0}")]
    ApiError(String),

    /// HTTP transport failure.
    #[error("QRZ Logbook network error: {0}")]
    NetworkError(#[from] reqwest::Error),

    /// Response body could not be parsed as expected.
    #[error("QRZ Logbook parse error: {0}")]
    ParseError(String),

    /// The server returned HTTP 429 Too Many Requests.
    #[error("QRZ Logbook rate limited")]
    RateLimited,
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// Result of a successful STATUS call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrzLogbookStatus {
    /// Owner callsign of the logbook.
    pub owner: String,
    /// Number of QSOs in the logbook.
    pub qso_count: u32,
}

/// Result of a successful INSERT call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QrzUploadResult {
    /// QRZ-assigned logbook record identifier.
    pub logid: String,
}

// ---------------------------------------------------------------------------
// Response parsing helpers (pure functions, easily testable)
// ---------------------------------------------------------------------------

/// Parse a QRZ key-value response body (`KEY=VALUE&KEY=VALUE`) into a map.
///
/// QRZ sometimes returns values that contain `&` inside angle-bracket
/// delimiters, so we do a simple split on `&` and `=`. Fields whose values
/// are empty are still inserted.
fn parse_kv_response(body: &str) -> HashMap<String, String> {
    let trimmed = body.trim();
    let mut map = HashMap::new();
    for pair in trimmed.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            map.insert(key.to_uppercase(), value.to_string());
        }
    }
    map
}

/// Check a parsed key-value response for `RESULT=OK`.
///
/// Returns `Ok(map)` when the result is OK, or an appropriate
/// [`QrzLogbookError`] variant when it is not.
fn check_result(map: HashMap<String, String>) -> Result<HashMap<String, String>, QrzLogbookError> {
    match map.get("RESULT").map(String::as_str) {
        Some("OK") => Ok(map),
        Some("FAIL") => {
            let reason = map
                .get("REASON")
                .cloned()
                .unwrap_or_else(|| "unknown error".to_string());
            if is_auth_error(&reason) {
                Err(QrzLogbookError::AuthenticationFailed(reason))
            } else {
                Err(QrzLogbookError::ApiError(reason))
            }
        }
        Some(other) => Err(QrzLogbookError::ParseError(format!(
            "unexpected RESULT value: {other}"
        ))),
        None => Err(QrzLogbookError::ParseError(
            "response missing RESULT field".to_string(),
        )),
    }
}

/// Determine whether a QRZ error reason indicates an authentication problem.
fn is_auth_error(reason: &str) -> bool {
    let lower = reason.to_ascii_lowercase();
    lower.contains("invalid api key")
        || lower.contains("api key required")
        || lower.contains("access denied")
}

fn find_adif_marker_index(body: &str) -> Option<usize> {
    body.to_ascii_uppercase().find("ADIF=")
}

fn starts_with_result_header(body: &str) -> bool {
    body.trim_start()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("RESULT="))
}

/// Extract the ADIF payload from a QRZ FETCH response body.
///
/// QRZ FETCH responses use the format:
///   `RESULT=OK&COUNT=773&ADIF=<time_off:4>2328\n<qso_date_off:8>...`
///
/// The ADIF data starts immediately after `ADIF=` and runs to the end of
/// the body. We cannot use `parse_kv_response` to extract it because the
/// ADIF content contains `&` and `=` characters inside angle-bracket fields.
fn extract_adif_from_fetch_body(body: &str) -> Option<String> {
    let marker_pos = find_adif_marker_index(body)?;
    let start = marker_pos + "ADIF=".len();
    if start >= body.len() {
        return None;
    }
    Some(body[start..].to_string())
}

/// Decode HTML entities that QRZ may return for FETCH ADIF payloads.
fn decode_fetch_adif_payload(payload: &str) -> String {
    if !payload.contains('&') {
        return payload.to_string();
    }

    payload
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        // Decode ampersands last to avoid re-encoding other entities.
        .replace("&amp;", "&")
}

/// Ensure ADIF has an explicit `<EOH>` marker so the parser treats entries as
/// QSO records even when the payload starts with arbitrary field order.
fn ensure_adif_has_eoh(payload: &str) -> String {
    if payload.to_ascii_uppercase().contains("<EOH>") {
        payload.to_string()
    } else {
        format!("<EOH>\n{payload}")
    }
}

fn normalize_adif_record_markers(payload: &str) -> String {
    payload.replace("<eor>", "<EOR>").replace("<eoh>", "<EOH>")
}

async fn parse_adif_records_tolerantly(payload: &str) -> Vec<QsoRecord> {
    if !payload.to_ascii_uppercase().contains("<EOR>") {
        return Vec::new();
    }

    let mut parsed = Vec::new();
    for record_fragment in payload.split("<EOR>") {
        let record = record_fragment.trim();
        if record.is_empty() {
            continue;
        }

        let candidate = format!("<EOH>\n{record}<EOR>\n");
        if let Ok(mut qsos) =
            adif::parse_adi_qsos_without_header_detection(candidate.as_bytes()).await
        {
            parsed.append(&mut qsos);
        }
    }

    parsed
}

/// A remotely fetched QSO must have at minimum a callsign and timestamp to be
/// usable for matching and storage. Records that fail this check are artefacts
/// of ADIF header/trailer fragments and should be silently dropped.
fn is_syncable_qso(qso: &QsoRecord) -> bool {
    !qso.worked_callsign.trim().is_empty() && qso.utc_timestamp.is_some()
}

async fn parse_fetch_adif_payload(
    adif_payload: &str,
    expected_count: Option<usize>,
) -> Result<Vec<QsoRecord>, QrzLogbookError> {
    let decoded_adif = decode_fetch_adif_payload(adif_payload);
    let normalized_markers = normalize_adif_record_markers(&decoded_adif);
    let normalized_adif = ensure_adif_has_eoh(&normalized_markers);

    let strict_result = adif::parse_adi_qsos(normalized_adif.as_bytes()).await;
    let strict_parse_error = match strict_result {
        Ok(qsos) if !qsos.is_empty() => {
            // Filter out ghost records (empty callsign / missing timestamp)
            // *before* comparing against the expected count so that trailing
            // ADIF artefacts don't needlessly trigger the tolerant fallback.
            let valid: Vec<QsoRecord> = qsos.into_iter().filter(is_syncable_qso).collect();
            let over_parsed =
                expected_count.is_some_and(|expected| expected > 0 && valid.len() > expected);
            if !valid.is_empty() && !over_parsed {
                return Ok(valid);
            }
            None
        }
        Ok(_) => None,
        Err(err) => Some(err),
    };

    let tolerant = parse_adif_records_tolerantly(&normalized_adif).await;
    let valid_tolerant: Vec<QsoRecord> = tolerant.into_iter().filter(is_syncable_qso).collect();
    if !valid_tolerant.is_empty() {
        return Ok(valid_tolerant);
    }

    if let Some(err) = strict_parse_error {
        return Err(QrzLogbookError::ParseError(err));
    }
    Ok(Vec::new())
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// HTTP client for the QRZ Logbook API.
#[derive(Debug)]
pub struct QrzLogbookClient {
    config: QrzLogbookConfig,
    client: Client,
}

impl QrzLogbookClient {
    /// Create a new client from validated configuration.
    ///
    /// # Errors
    ///
    /// Returns [`QrzLogbookError::NetworkError`] if the underlying HTTP client
    /// cannot be built.
    pub fn new(config: QrzLogbookConfig) -> Result<Self, QrzLogbookError> {
        let client = Client::builder()
            .user_agent(config.user_agent.clone())
            .timeout(config.http_timeout)
            .build()?;
        Ok(Self { config, client })
    }

    /// Test the connection and return logbook status.
    ///
    /// Calls the QRZ Logbook `STATUS` action.
    ///
    /// # Errors
    ///
    /// Returns an error when authentication fails, the API returns an error,
    /// or the response cannot be parsed.
    pub async fn test_connection(&self) -> Result<QrzLogbookStatus, QrzLogbookError> {
        let body = self.post_form(&[("ACTION", "STATUS")]).await?;
        let map = parse_kv_response(&body);
        let map = check_result(map)?;

        let owner = map
            .get("CALLSIGN")
            .or_else(|| map.get("OWNER"))
            .cloned()
            .unwrap_or_default();
        let qso_count = map
            .get("COUNT")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(0);

        Ok(QrzLogbookStatus { owner, qso_count })
    }

    /// Fetch QSO records from the QRZ Logbook.
    ///
    /// When `since` is `Some("YYYY-MM-DD")`, only QSOs modified after that
    /// date are returned. Otherwise all QSOs are fetched.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure, authentication failure, or ADIF
    /// parse failure.
    pub async fn fetch_qsos(&self, since: Option<&str>) -> Result<Vec<QsoRecord>, QrzLogbookError> {
        let option_value = match since {
            Some(date) => format!("MODSINCE:{date}"),
            None => "ALL".to_string(),
        };

        let body = self
            .post_form(&[("ACTION", "FETCH"), ("OPTION", &option_value)])
            .await?;

        // Some error/empty FETCH responses are key-value only (no ADIF field).
        if starts_with_result_header(&body) && find_adif_marker_index(&body).is_none() {
            let map = parse_kv_response(&body);
            check_result(map)?;
            return Ok(Vec::new());
        }

        // QRZ FETCH responses commonly use:
        //   RESULT=OK&COUNT=N&ADIF=<adif data...>
        // Detect that shape by splitting at ADIF= and checking for RESULT in
        // the prefix.
        if let Some(marker_pos) = find_adif_marker_index(&body) {
            let header = &body[..marker_pos];
            let map = parse_kv_response(header);
            if map.contains_key("RESULT") {
                let map = check_result(map)?;
                let expected_count = map
                    .get("COUNT")
                    .and_then(|value| value.parse::<usize>().ok());
                let adif = extract_adif_from_fetch_body(&body).unwrap_or_default();
                if !adif.trim().is_empty() {
                    return parse_fetch_adif_payload(&adif, expected_count).await;
                }
                return Ok(Vec::new());
            }
        }

        adif::parse_adi_qsos(body.as_bytes())
            .await
            .map_err(QrzLogbookError::ParseError)
    }

    /// Upload a single QSO to the QRZ Logbook.
    ///
    /// The QSO is serialized to an ADIF record string and sent via the
    /// `INSERT` action.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure, authentication failure, or if
    /// the QRZ API rejects the record.
    pub async fn upload_qso(&self, qso: &QsoRecord) -> Result<QrzUploadResult, QrzLogbookError> {
        let adif_record = AdifMapper::qso_to_adi(qso);

        let body = self
            .post_form(&[("ACTION", "INSERT"), ("ADIF", &adif_record)])
            .await?;
        let map = parse_kv_response(&body);
        let map = check_result(map)?;

        let logid = map.get("LOGID").cloned().unwrap_or_default();
        if logid.is_empty() {
            return Err(QrzLogbookError::ParseError(
                "INSERT response missing LOGID".to_string(),
            ));
        }

        Ok(QrzUploadResult { logid })
    }

    /// Delete a QSO from the QRZ Logbook by its logbook record ID.
    ///
    /// # Errors
    ///
    /// Returns an error on network failure, authentication failure, or if
    /// the QRZ API rejects the deletion.
    pub async fn delete_qso(&self, logid: &str) -> Result<(), QrzLogbookError> {
        let body = self
            .post_form(&[("ACTION", "DELETE"), ("LOGID", logid)])
            .await?;
        let map = parse_kv_response(&body);
        check_result(map)?;
        Ok(())
    }

    /// Send a form-encoded POST to the QRZ Logbook API.
    ///
    /// Every request includes the API key. Returns the response body as text,
    /// or an appropriate error for HTTP-level failures.
    async fn post_form(&self, params: &[(&str, &str)]) -> Result<String, QrzLogbookError> {
        let mut form: Vec<(&str, &str)> = vec![("KEY", &self.config.api_key)];
        form.extend_from_slice(params);

        let response = self
            .client
            .post(&self.config.base_url)
            .form(&form)
            .send()
            .await?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(QrzLogbookError::RateLimited);
        }

        if !status.is_success() {
            return Err(QrzLogbookError::ApiError(format!("HTTP {status}")));
        }

        response.text().await.map_err(QrzLogbookError::NetworkError)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]
mod tests {
    use super::*;

    // -- Response parsing helpers -------------------------------------------

    #[test]
    fn parse_kv_response_extracts_fields() {
        let body = "RESULT=OK&COUNT=42&CALLSIGN=W1AW";
        let map = parse_kv_response(body);
        assert_eq!(map.get("RESULT").unwrap(), "OK");
        assert_eq!(map.get("COUNT").unwrap(), "42");
        assert_eq!(map.get("CALLSIGN").unwrap(), "W1AW");
    }

    #[test]
    fn parse_kv_response_uppercases_keys() {
        let body = "result=OK&count=7";
        let map = parse_kv_response(body);
        assert_eq!(map.get("RESULT").unwrap(), "OK");
        assert_eq!(map.get("COUNT").unwrap(), "7");
    }

    #[test]
    fn parse_kv_response_handles_empty_body() {
        let map = parse_kv_response("");
        assert!(map.is_empty());
    }

    #[test]
    fn parse_kv_response_trims_whitespace() {
        let body = "  RESULT=OK&COUNT=1  \n";
        let map = parse_kv_response(body);
        assert_eq!(map.get("RESULT").unwrap(), "OK");
    }

    #[test]
    fn check_result_ok_returns_map() {
        let map = parse_kv_response("RESULT=OK&LOGID=12345");
        let result = check_result(map).unwrap();
        assert_eq!(result.get("LOGID").unwrap(), "12345");
    }

    #[test]
    fn check_result_fail_returns_api_error() {
        let map = parse_kv_response("RESULT=FAIL&REASON=bad record format");
        let err = check_result(map).unwrap_err();
        match err {
            QrzLogbookError::ApiError(reason) => assert_eq!(reason, "bad record format"),
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    #[test]
    fn check_result_fail_auth_error() {
        let map = parse_kv_response("RESULT=FAIL&REASON=invalid api key");
        let err = check_result(map).unwrap_err();
        assert!(matches!(err, QrzLogbookError::AuthenticationFailed(_)));
    }

    #[test]
    fn check_result_missing_result_field() {
        let map = parse_kv_response("LOGID=12345");
        let err = check_result(map).unwrap_err();
        assert!(matches!(err, QrzLogbookError::ParseError(_)));
    }

    #[test]
    fn check_result_unexpected_result_value() {
        let map = parse_kv_response("RESULT=MAYBE");
        let err = check_result(map).unwrap_err();
        assert!(matches!(err, QrzLogbookError::ParseError(_)));
    }

    #[test]
    fn check_result_fail_without_reason_uses_default() {
        let map = parse_kv_response("RESULT=FAIL");
        let err = check_result(map).unwrap_err();
        match err {
            QrzLogbookError::ApiError(reason) => assert_eq!(reason, "unknown error"),
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    #[test]
    fn is_auth_error_detects_common_messages() {
        assert!(is_auth_error("invalid api key"));
        assert!(is_auth_error("Invalid API Key"));
        assert!(is_auth_error("API key required"));
        assert!(is_auth_error("Access Denied"));
        assert!(!is_auth_error("bad record format"));
        assert!(!is_auth_error("duplicate QSO"));
    }

    // -- Status parsing -----------------------------------------------------

    #[test]
    fn status_parsing_extracts_owner_and_count() {
        let body = "RESULT=OK&CALLSIGN=KC7AVA&COUNT=1234";
        let map = parse_kv_response(body);
        let map = check_result(map).unwrap();

        let owner = map
            .get("CALLSIGN")
            .or_else(|| map.get("OWNER"))
            .cloned()
            .unwrap_or_default();
        let qso_count = map
            .get("COUNT")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);

        assert_eq!(owner, "KC7AVA");
        assert_eq!(qso_count, 1234);
    }

    #[test]
    fn status_parsing_uses_owner_field_as_fallback() {
        let body = "RESULT=OK&OWNER=N0CALL&COUNT=0";
        let map = parse_kv_response(body);
        let map = check_result(map).unwrap();

        let owner = map
            .get("CALLSIGN")
            .or_else(|| map.get("OWNER"))
            .cloned()
            .unwrap_or_default();

        assert_eq!(owner, "N0CALL");
    }

    #[test]
    fn status_parsing_defaults_count_when_missing() {
        let body = "RESULT=OK&CALLSIGN=W1AW";
        let map = parse_kv_response(body);
        let map = check_result(map).unwrap();

        let qso_count = map
            .get("COUNT")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);

        assert_eq!(qso_count, 0);
    }

    // -- Insert response parsing --------------------------------------------

    #[test]
    fn insert_response_extracts_logid() {
        let body = "RESULT=OK&LOGID=987654&LOGIDS=987654";
        let map = parse_kv_response(body);
        let map = check_result(map).unwrap();
        let logid = map.get("LOGID").cloned().unwrap_or_default();
        assert_eq!(logid, "987654");
    }

    #[test]
    fn insert_response_missing_logid_is_error() {
        let body = "RESULT=OK";
        let map = parse_kv_response(body);
        let map = check_result(map).unwrap();
        let logid = map.get("LOGID").cloned().unwrap_or_default();
        assert!(logid.is_empty(), "expected empty logid");
    }

    // -- ADIF pipeline tests ------------------------------------------------

    #[test]
    fn qso_serializes_to_adif_record_for_upload() {
        let qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            ..Default::default()
        };
        let adif = AdifMapper::qso_to_adi(&qso);
        assert!(adif.contains("<CALL:4>W1AW"));
        assert!(adif.contains("<eor>"));
    }

    #[tokio::test]
    async fn adif_round_trip_through_parse() {
        let qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            ..Default::default()
        };
        let adif_bytes = adif::serialize_adi_qsos(&[qso], false);
        let parsed = adif::parse_adi_qsos(&adif_bytes)
            .await
            .expect("round-trip parse");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].worked_callsign, "W1AW");
    }

    // -- Config tests -------------------------------------------------------

    #[test]
    fn config_debug_redacts_api_key() {
        let config = QrzLogbookConfig::new(
            "secret-key".to_string(),
            DEFAULT_QRZ_LOGBOOK_BASE_URL.to_string(),
            DEFAULT_USER_AGENT.to_string(),
        );
        let debug = format!("{config:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret-key"));
    }

    #[test]
    fn config_new_sets_defaults() {
        let config = QrzLogbookConfig::new(
            "key".to_string(),
            "http://localhost".to_string(),
            "Agent/1.0".to_string(),
        );
        assert_eq!(config.base_url(), "http://localhost");
        assert_eq!(
            config.http_timeout,
            Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECONDS)
        );
    }

    // -- Integration-level tests using a local TCP server -------------------

    use std::sync::{Arc, Mutex as StdMutex};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::{TcpListener, TcpStream},
    };

    fn test_config(base_url: String) -> QrzLogbookConfig {
        QrzLogbookConfig {
            api_key: "test-api-key".to_string(),
            base_url,
            user_agent: "QsoRipper/test".to_string(),
            http_timeout: Duration::from_secs(2),
        }
    }

    /// Spawn a minimal HTTP server that serves pre-canned responses in order.
    async fn spawn_logbook_server(
        responses: &[(&str, &str)],
    ) -> (String, Arc<StdMutex<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");
        let recorded_requests = Arc::new(StdMutex::new(Vec::new()));
        let recorded_clone = Arc::clone(&recorded_requests);
        let responses: Vec<(String, String)> = responses
            .iter()
            .map(|(ct, body)| ((*ct).to_string(), (*body).to_string()))
            .collect();

        tokio::spawn(async move {
            for (content_type, response_body) in responses {
                let (mut socket, _) = listener.accept().await.expect("accept");
                let request = read_http_request(&mut socket).await;
                recorded_clone
                    .lock()
                    .expect("recorded requests")
                    .push(request);
                write_http_response(&mut socket, &content_type, &response_body).await;
            }
        });

        (format!("http://{address}/api"), recorded_requests)
    }

    async fn read_http_request(socket: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut content_length: Option<usize> = None;

        loop {
            let read = socket.read(&mut buffer).await.expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(buffer.get(..read).expect("buffer slice"));

            // Check if we've received all headers.
            if let Some(pos) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let header_end = pos + 4;
                // Extract Content-Length from headers.
                let header_str =
                    String::from_utf8_lossy(bytes.get(..header_end).unwrap_or_default());
                for line in header_str.lines() {
                    if let Some(value) = line.strip_prefix("Content-Length: ") {
                        content_length = value.trim().parse().ok();
                    }
                    // Also handle lowercase
                    if let Some(value) = line.strip_prefix("content-length: ") {
                        content_length = value.trim().parse().ok();
                    }
                }

                let expected_total = header_end + content_length.unwrap_or(0);
                if bytes.len() >= expected_total {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&bytes).into_owned()
    }

    async fn write_http_response(socket: &mut TcpStream, content_type: &str, body: &str) {
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    }

    async fn write_http_response_with_status(
        socket: &mut TcpStream,
        status_code: u16,
        status_text: &str,
        body: &str,
    ) {
        let response = format!(
            "HTTP/1.1 {status_code} {status_text}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        socket
            .write_all(response.as_bytes())
            .await
            .expect("write response");
    }

    // -- test_connection integration ----------------------------------------

    #[tokio::test]
    async fn test_connection_success() {
        let (base_url, requests) =
            spawn_logbook_server(&[("text/plain", "RESULT=OK&CALLSIGN=KC7AVA&COUNT=500")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let status = client.test_connection().await.expect("status");

        assert_eq!(status.owner, "KC7AVA");
        assert_eq!(status.qso_count, 500);

        let reqs = requests.lock().expect("requests");
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].contains("ACTION=STATUS"));
        assert!(reqs[0].contains("KEY=test-api-key"));
    }

    #[tokio::test]
    async fn test_connection_auth_failure() {
        let (base_url, _) =
            spawn_logbook_server(&[("text/plain", "RESULT=FAIL&REASON=invalid api key")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let err = client.test_connection().await.unwrap_err();

        assert!(
            matches!(err, QrzLogbookError::AuthenticationFailed(_)),
            "expected AuthenticationFailed, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_connection_api_error() {
        let (base_url, _) =
            spawn_logbook_server(&[("text/plain", "RESULT=FAIL&REASON=service unavailable")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let err = client.test_connection().await.unwrap_err();

        match err {
            QrzLogbookError::ApiError(reason) => {
                assert_eq!(reason, "service unavailable");
            }
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    // -- fetch_qsos integration ---------------------------------------------

    #[tokio::test]
    async fn fetch_qsos_parses_adif_response() {
        let adif_body = "<CALL:4>W1AW <BAND:3>20M <FREQ:6>14.250 <MODE:3>SSB <QSO_DATE:8>20250101 <TIME_ON:4>1200 <EOR>\n\
                         <CALL:6>N0CALL <BAND:3>40M <FREQ:5>7.200 <MODE:2>CW <QSO_DATE:8>20250102 <TIME_ON:4>1300 <EOR>\n";
        let (base_url, requests) = spawn_logbook_server(&[("text/plain", adif_body)]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(None).await.expect("fetch");

        assert_eq!(qsos.len(), 2);
        assert_eq!(qsos[0].worked_callsign, "W1AW");
        assert_eq!(qsos[1].worked_callsign, "N0CALL");

        let reqs = requests.lock().expect("requests");
        assert!(reqs[0].contains("ACTION=FETCH"));
        assert!(reqs[0].contains("OPTION=ALL"));
    }

    #[tokio::test]
    async fn fetch_qsos_sends_modsince_option() {
        let adif_body = "<CALL:4>W1AW <EOR>\n";
        let (base_url, requests) = spawn_logbook_server(&[("text/plain", adif_body)]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(Some("2025-06-01")).await.expect("fetch");

        assert_eq!(qsos.len(), 1);
        let reqs = requests.lock().expect("requests");
        // URL-encoded colon: %3A
        assert!(
            reqs[0].contains("MODSINCE") && reqs[0].contains("2025-06-01"),
            "expected MODSINCE with date in request: {}",
            reqs[0]
        );
    }

    #[tokio::test]
    async fn fetch_qsos_handles_kv_error_response() {
        let (base_url, _) =
            spawn_logbook_server(&[("text/plain", "RESULT=FAIL&REASON=invalid api key")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let err = client.fetch_qsos(None).await.unwrap_err();

        assert!(
            matches!(err, QrzLogbookError::AuthenticationFailed(_)),
            "expected AuthenticationFailed, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn fetch_qsos_returns_empty_for_ok_with_no_adif() {
        let (base_url, _) = spawn_logbook_server(&[("text/plain", "RESULT=OK")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(None).await.expect("fetch");

        assert!(qsos.is_empty());
    }

    #[tokio::test]
    async fn fetch_qsos_parses_result_ok_with_inline_adif() {
        // Real QRZ FETCH format: RESULT=OK&COUNT=N&ADIF=<adif records...>
        let body = "RESULT=OK&COUNT=2&ADIF=<CALL:4>W1AW <BAND:3>20M <MODE:3>SSB \
                    <QSO_DATE:8>20250101 <TIME_ON:4>1200 <EOR>\n\
                    <CALL:6>N0CALL <BAND:3>40M <MODE:2>CW \
                    <QSO_DATE:8>20250102 <TIME_ON:4>1300 <EOR>\n";
        let (base_url, _) = spawn_logbook_server(&[("text/plain", body)]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(None).await.expect("fetch");

        assert_eq!(qsos.len(), 2, "expected 2 QSOs from inline ADIF");
        assert_eq!(qsos[0].worked_callsign, "W1AW");
        assert_eq!(qsos[1].worked_callsign, "N0CALL");
    }

    #[tokio::test]
    async fn fetch_qsos_decodes_html_encoded_inline_adif() {
        let body = "RESULT=OK&COUNT=1&ADIF=&lt;CALL:4&gt;W1AW &lt;BAND:3&gt;20M \
                    &lt;MODE:3&gt;SSB &lt;QSO_DATE:8&gt;20250101 &lt;TIME_ON:4&gt;1200 \
                    &lt;EOR&gt;\n";
        let (base_url, _) = spawn_logbook_server(&[("text/plain", body)]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(None).await.expect("fetch");

        assert_eq!(qsos.len(), 1, "expected one decoded QSO");
        assert_eq!(qsos[0].worked_callsign, "W1AW");
    }

    #[tokio::test]
    async fn fetch_qsos_decodes_html_encoded_lowercase_eor_markers() {
        let body = "RESULT=OK&COUNT=1&ADIF=&lt;CALL:4&gt;W1AW &lt;BAND:3&gt;20M \
                    &lt;MODE:3&gt;SSB &lt;QSO_DATE:8&gt;20250101 &lt;TIME_ON:4&gt;1200 \
                    &lt;eor&gt;\n";
        let (base_url, _) = spawn_logbook_server(&[("text/plain", body)]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(None).await.expect("fetch");

        assert_eq!(qsos.len(), 1, "expected one decoded QSO");
        assert_eq!(qsos[0].worked_callsign, "W1AW");
    }

    #[tokio::test]
    async fn fetch_qsos_parses_inline_adif_without_eoh_and_non_call_first_field() {
        // QRZ often starts each record with fields like DXCC/FREQ before CALL.
        let body = "RESULT=OK&COUNT=1&ADIF=<DXCC:3>339 <FREQ:6>28.405 <CALL:4>W1AW \
                    <BAND:3>10M <MODE:3>SSB <QSO_DATE:8>20250101 <TIME_ON:4>1200 <EOR>\n";
        let (base_url, _) = spawn_logbook_server(&[("text/plain", body)]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qsos = client.fetch_qsos(None).await.expect("fetch");

        assert_eq!(qsos.len(), 1, "expected one QSO from non-CALL-first record");
        assert_eq!(qsos[0].worked_callsign, "W1AW");
    }

    #[test]
    fn extract_adif_from_fetch_body_extracts_content() {
        let body = "RESULT=OK&COUNT=1&ADIF=<CALL:4>W1AW <EOR>\n";
        let adif = extract_adif_from_fetch_body(body);
        assert_eq!(adif.as_deref(), Some("<CALL:4>W1AW <EOR>\n"));
    }

    #[test]
    fn extract_adif_from_fetch_body_returns_none_when_missing() {
        let body = "RESULT=OK&COUNT=0";
        let adif = extract_adif_from_fetch_body(body);
        assert!(adif.is_none());
    }

    #[test]
    fn extract_adif_from_fetch_body_case_insensitive() {
        let body = "RESULT=OK&COUNT=1&adif=<CALL:4>W1AW <EOR>\n";
        let adif = extract_adif_from_fetch_body(body);
        assert_eq!(adif.as_deref(), Some("<CALL:4>W1AW <EOR>\n"));
    }

    #[test]
    fn decode_fetch_adif_payload_decodes_entities() {
        let encoded = "&lt;CALL:4&gt;W1AW &lt;EOR&gt; &amp; &quot;OK&quot; &#39;QSO&#39;";
        let decoded = decode_fetch_adif_payload(encoded);
        assert_eq!(decoded, "<CALL:4>W1AW <EOR> & \"OK\" 'QSO'");
    }

    #[test]
    fn ensure_adif_has_eoh_adds_header_marker_when_missing() {
        let payload = "<CALL:4>W1AW <EOR>\n";
        let normalized = ensure_adif_has_eoh(payload);
        assert!(normalized.starts_with("<EOH>\n"));
    }

    #[test]
    fn ensure_adif_has_eoh_preserves_existing_marker() {
        let payload = "<ADIF_VER:5>3.1.0\n<EOH>\n<CALL:4>W1AW <EOR>\n";
        let normalized = ensure_adif_has_eoh(payload);
        assert_eq!(normalized, payload);
    }

    #[test]
    fn normalize_adif_record_markers_converts_lowercase_markers() {
        let payload = "<CALL:4>W1AW <eor>\n<eoh>\n";
        let normalized = normalize_adif_record_markers(payload);
        assert_eq!(normalized, "<CALL:4>W1AW <EOR>\n<EOH>\n");
    }

    // -- is_syncable_qso / ghost filtering -----------------------------------

    #[test]
    fn is_syncable_qso_rejects_empty_callsign() {
        let ghost = QsoRecord::default();
        assert!(!is_syncable_qso(&ghost));
    }

    #[test]
    fn is_syncable_qso_rejects_missing_timestamp() {
        let qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            utc_timestamp: None,
            ..Default::default()
        };
        assert!(!is_syncable_qso(&qso));
    }

    #[test]
    fn is_syncable_qso_accepts_valid_record() {
        let qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            utc_timestamp: Some(prost_types::Timestamp {
                seconds: 1_700_000_000,
                nanos: 0,
            }),
            ..Default::default()
        };
        assert!(is_syncable_qso(&qso));
    }

    #[tokio::test]
    async fn parse_fetch_adif_drops_ghost_records_from_trailing_content() {
        // Simulate QRZ response with 2 real records + trailing junk after last <EOR>
        let adif = "<CALL:4>W1AW <BAND:3>20M <MODE:3>SSB \
                    <QSO_DATE:8>20250101 <TIME_ON:4>1200 <EOR>\n\
                    <CALL:6>N0CALL <BAND:3>40M <MODE:2>CW \
                    <QSO_DATE:8>20250102 <TIME_ON:4>1300 <EOR>\n\
                    some trailing garbage\n";

        let result = parse_fetch_adif_payload(adif, Some(2))
            .await
            .expect("parse");

        assert_eq!(result.len(), 2, "ghost records should be filtered out");
        assert_eq!(result[0].worked_callsign, "W1AW");
        assert_eq!(result[1].worked_callsign, "N0CALL");
    }

    #[tokio::test]
    async fn parse_fetch_adif_count_mismatch_does_not_trigger_tolerant_after_filtering() {
        // Strict parser may produce 3 records (2 real + 1 ghost), but after
        // filtering, the 2 real records match COUNT=2 and should be returned
        // without falling through to the tolerant path.
        let adif = "<CALL:4>W1AW <BAND:3>20M <MODE:3>SSB \
                    <QSO_DATE:8>20250101 <TIME_ON:4>1200 <EOR>\n\
                    <CALL:6>N0CALL <BAND:3>40M <MODE:2>CW \
                    <QSO_DATE:8>20250102 <TIME_ON:4>1300 <EOR>\n";

        let result = parse_fetch_adif_payload(adif, Some(2))
            .await
            .expect("parse");

        assert_eq!(result.len(), 2);
    }

    // -- upload_qso integration ---------------------------------------------

    #[tokio::test]
    async fn upload_qso_sends_adif_and_returns_logid() {
        let (base_url, requests) =
            spawn_logbook_server(&[("text/plain", "RESULT=OK&LOGID=999888&LOGIDS=999888")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            ..Default::default()
        };
        let result = client.upload_qso(&qso).await.expect("upload");

        assert_eq!(result.logid, "999888");

        let reqs = requests.lock().expect("requests");
        assert!(reqs[0].contains("ACTION=INSERT"));
        // The ADIF record should be in the form body (URL-encoded)
        assert!(reqs[0].contains("ADIF="));
    }

    #[tokio::test]
    async fn upload_qso_returns_error_on_missing_logid() {
        let (base_url, _) = spawn_logbook_server(&[("text/plain", "RESULT=OK")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qso = QsoRecord {
            worked_callsign: "W1AW".to_string(),
            ..Default::default()
        };
        let err = client.upload_qso(&qso).await.unwrap_err();

        assert!(
            matches!(err, QrzLogbookError::ParseError(_)),
            "expected ParseError, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn upload_qso_surfaces_api_failure() {
        let (base_url, _) =
            spawn_logbook_server(&[("text/plain", "RESULT=FAIL&REASON=duplicate QSO")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let qso = QsoRecord::default();
        let err = client.upload_qso(&qso).await.unwrap_err();

        match err {
            QrzLogbookError::ApiError(reason) => assert_eq!(reason, "duplicate QSO"),
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    // -- delete_qso integration ---------------------------------------------

    #[tokio::test]
    async fn delete_qso_success() {
        let (base_url, requests) = spawn_logbook_server(&[("text/plain", "RESULT=OK")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        client.delete_qso("123456").await.expect("delete");

        let reqs = requests.lock().expect("requests");
        assert!(reqs[0].contains("ACTION=DELETE"));
        assert!(reqs[0].contains("LOGID=123456"));
    }

    #[tokio::test]
    async fn delete_qso_api_failure() {
        let (base_url, _) =
            spawn_logbook_server(&[("text/plain", "RESULT=FAIL&REASON=record not found")]).await;
        let client = QrzLogbookClient::new(test_config(base_url)).expect("client");

        let err = client.delete_qso("000000").await.unwrap_err();

        match err {
            QrzLogbookError::ApiError(reason) => assert_eq!(reason, "record not found"),
            other => panic!("expected ApiError, got: {other:?}"),
        }
    }

    // -- Rate limiting ------------------------------------------------------

    #[tokio::test]
    async fn rate_limited_response_returns_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("local addr");

        tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buffer = [0_u8; 4096];
            // Read the full request.
            loop {
                let n = socket.read(&mut buffer).await.expect("read");
                if n == 0 {
                    break;
                }
                if buffer
                    .get(..n)
                    .unwrap_or_default()
                    .windows(4)
                    .any(|w| w == b"\r\n\r\n")
                {
                    break;
                }
            }
            write_http_response_with_status(&mut socket, 429, "Too Many Requests", "").await;
        });

        let config = test_config(format!("http://{address}/api"));
        let client = QrzLogbookClient::new(config).expect("client");

        let err = client.test_connection().await.unwrap_err();

        assert!(
            matches!(err, QrzLogbookError::RateLimited),
            "expected RateLimited, got: {err:?}"
        );
    }
}
