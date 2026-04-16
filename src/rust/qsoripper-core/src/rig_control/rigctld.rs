//! rigctld TCP adapter for reading live rig state.

use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::proto::qsoripper::domain::{RigConnectionStatus, RigSnapshot};

use super::band_mapping::frequency_hz_to_band;
use super::mode_mapping::hamlib_mode_to_proto;
use super::provider::{RigControlProvider, RigControlProviderError};

/// Default rigctld host.
pub const DEFAULT_RIGCTLD_HOST: &str = "127.0.0.1";

/// Default rigctld TCP port.
pub const DEFAULT_RIGCTLD_PORT: u16 = 4532;

/// Default read timeout in milliseconds.
pub const DEFAULT_RIGCTLD_READ_TIMEOUT_MS: u64 = 2_000;

/// Environment variable to enable/disable rigctld integration.
pub const RIGCTLD_ENABLED_ENV_VAR: &str = "QSORIPPER_RIGCTLD_ENABLED";
/// Environment variable for rigctld host address.
pub const RIGCTLD_HOST_ENV_VAR: &str = "QSORIPPER_RIGCTLD_HOST";
/// Environment variable for rigctld TCP port.
pub const RIGCTLD_PORT_ENV_VAR: &str = "QSORIPPER_RIGCTLD_PORT";
/// Environment variable for rigctld read timeout in milliseconds.
pub const RIGCTLD_READ_TIMEOUT_MS_ENV_VAR: &str = "QSORIPPER_RIGCTLD_READ_TIMEOUT_MS";
/// Environment variable for rig snapshot stale threshold in milliseconds.
pub const RIGCTLD_STALE_THRESHOLD_MS_ENV_VAR: &str = "QSORIPPER_RIGCTLD_STALE_THRESHOLD_MS";

/// Default stale threshold in milliseconds.
pub const DEFAULT_RIGCTLD_STALE_THRESHOLD_MS: u64 = 500;

/// Configuration for the rigctld adapter.
#[derive(Debug, Clone)]
pub struct RigctldConfig {
    /// rigctld host address.
    pub host: String,
    /// rigctld TCP port.
    pub port: u16,
    /// Read timeout for TCP operations.
    pub read_timeout: Duration,
}

impl RigctldConfig {
    /// Build configuration from a value provider closure.
    ///
    /// Returns `None` if rig control is not enabled.
    pub fn from_value_provider(get_value: impl Fn(&str) -> Option<String>) -> Option<Self> {
        let enabled = get_value(RIGCTLD_ENABLED_ENV_VAR)
            .is_none_or(|value| value.eq_ignore_ascii_case("true") || value == "1");

        if !enabled {
            return None;
        }

        let host =
            get_value(RIGCTLD_HOST_ENV_VAR).unwrap_or_else(|| DEFAULT_RIGCTLD_HOST.to_string());

        let port = get_value(RIGCTLD_PORT_ENV_VAR)
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_RIGCTLD_PORT);

        let read_timeout_ms = get_value(RIGCTLD_READ_TIMEOUT_MS_ENV_VAR)
            .and_then(|value| value.parse().ok())
            .unwrap_or(DEFAULT_RIGCTLD_READ_TIMEOUT_MS);

        Some(Self {
            host,
            port,
            read_timeout: Duration::from_millis(read_timeout_ms),
        })
    }
}

/// rigctld-backed rig control provider.
///
/// Connects to a running `rigctld` daemon over TCP, reads frequency and mode,
/// and normalizes the result into project-owned proto types.
///
/// Each [`get_snapshot`] call opens a fresh TCP connection, issues both
/// commands on the same socket, and closes it. This avoids stale-connection
/// bugs while keeping the protocol exchange atomic.
pub struct RigctldProvider {
    config: RigctldConfig,
}

impl RigctldProvider {
    /// Create a provider with the given configuration.
    #[must_use]
    pub fn new(config: RigctldConfig) -> Self {
        Self { config }
    }

    /// Read frequency and mode from rigctld on a single TCP connection.
    async fn read_rig_state(&self) -> Result<(u64, String), RigControlProviderError> {
        let address = format!("{}:{}", self.config.host, self.config.port);

        let stream = timeout(self.config.read_timeout, TcpStream::connect(&address))
            .await
            .map_err(|_| {
                RigControlProviderError::timeout(format!(
                    "Connection to {address} timed out after {:?}",
                    self.config.read_timeout
                ))
            })?
            .map_err(|error| {
                RigControlProviderError::transport(format!(
                    "Failed to connect to {address}: {error}"
                ))
            })?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Read frequency: send "f\n", expect one line with Hz value
        writer.write_all(b"f\n").await.map_err(|error| {
            RigControlProviderError::transport(format!("Failed to send frequency command: {error}"))
        })?;

        let freq_line = read_line_with_timeout(&mut reader, self.config.read_timeout).await?;
        let frequency_hz = parse_frequency(&freq_line)?;

        // Read mode: send "m\n", expect two lines (mode string, passband)
        writer.write_all(b"m\n").await.map_err(|error| {
            RigControlProviderError::transport(format!("Failed to send mode command: {error}"))
        })?;

        let mode_line = read_line_with_timeout(&mut reader, self.config.read_timeout).await?;
        // Read and discard passband line
        let _passband = read_line_with_timeout(&mut reader, self.config.read_timeout).await;

        Ok((frequency_hz, mode_line))
    }
}

#[tonic::async_trait]
impl RigControlProvider for RigctldProvider {
    async fn get_snapshot(&self) -> Result<RigSnapshot, RigControlProviderError> {
        let (frequency_hz, raw_mode) = self.read_rig_state().await?;
        let band = frequency_hz_to_band(frequency_hz);
        let mode_mapping = hamlib_mode_to_proto(&raw_mode);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();

        Ok(RigSnapshot {
            frequency_hz,
            band: band as i32,
            mode: mode_mapping.mode as i32,
            submode: mode_mapping.submode,
            raw_mode: Some(raw_mode),
            status: RigConnectionStatus::Connected as i32,
            error_message: None,
            sampled_at: Some(prost_types::Timestamp {
                seconds: i64::try_from(now.as_secs()).unwrap_or(i64::MAX),
                nanos: i32::try_from(now.subsec_nanos()).unwrap_or(i32::MAX),
            }),
        })
    }
}

async fn read_line_with_timeout<R: tokio::io::AsyncBufRead + Unpin>(
    reader: &mut R,
    read_timeout: Duration,
) -> Result<String, RigControlProviderError> {
    let mut line = String::new();
    timeout(read_timeout, reader.read_line(&mut line))
        .await
        .map_err(|_| RigControlProviderError::timeout("Timed out reading from rigctld"))?
        .map_err(|error| {
            RigControlProviderError::transport(format!("Failed to read from rigctld: {error}"))
        })?;

    let trimmed = line.trim().to_string();

    // rigctld reports errors as "RPRT -N" where N is the error code
    if trimmed.starts_with("RPRT") {
        return Err(RigControlProviderError::parse(format!(
            "rigctld error: {trimmed}"
        )));
    }

    Ok(trimmed)
}

fn parse_frequency(line: &str) -> Result<u64, RigControlProviderError> {
    line.trim().parse::<u64>().map_err(|error| {
        RigControlProviderError::parse(format!("Invalid frequency value '{line}': {error}"))
    })
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::rig_control::provider::RigControlProviderErrorKind;
    use tokio::io::AsyncWriteExt;
    use tokio::net::TcpListener;

    async fn start_fake_rigctld(
        frequency_hz: &str,
        mode: &str,
        passband: &str,
    ) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let freq = frequency_hz.to_string();
        let mode = mode.to_string();
        let passband = passband.to_string();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);

            // Handle "f\n" command
            let mut cmd = String::new();
            reader.read_line(&mut cmd).await.unwrap();
            assert_eq!("f\n", cmd);
            writer
                .write_all(format!("{freq}\n").as_bytes())
                .await
                .unwrap();

            // Handle "m\n" command
            cmd.clear();
            reader.read_line(&mut cmd).await.unwrap();
            assert_eq!("m\n", cmd);
            writer
                .write_all(format!("{mode}\n{passband}\n").as_bytes())
                .await
                .unwrap();
        });

        addr
    }

    #[tokio::test]
    async fn reads_frequency_and_mode_from_rigctld() {
        let addr = start_fake_rigctld("14074000", "USB", "2400").await;

        let provider = RigctldProvider::new(RigctldConfig {
            host: addr.ip().to_string(),
            port: addr.port(),
            read_timeout: Duration::from_secs(2),
        });

        let snapshot = provider.get_snapshot().await.expect("snapshot");

        assert_eq!(14_074_000, snapshot.frequency_hz);
        assert_eq!(
            crate::proto::qsoripper::domain::Band::Band20m as i32,
            snapshot.band
        );
        assert_eq!(
            crate::proto::qsoripper::domain::Mode::Ssb as i32,
            snapshot.mode
        );
        assert_eq!(Some("USB".to_string()), snapshot.submode);
        assert_eq!(Some("USB".to_string()), snapshot.raw_mode);
        assert_eq!(RigConnectionStatus::Connected as i32, snapshot.status);
    }

    #[tokio::test]
    async fn reads_cw_mode() {
        let addr = start_fake_rigctld("7030000", "CW", "500").await;

        let provider = RigctldProvider::new(RigctldConfig {
            host: addr.ip().to_string(),
            port: addr.port(),
            read_timeout: Duration::from_secs(2),
        });

        let snapshot = provider.get_snapshot().await.expect("snapshot");

        assert_eq!(7_030_000, snapshot.frequency_hz);
        assert_eq!(
            crate::proto::qsoripper::domain::Band::Band40m as i32,
            snapshot.band
        );
        assert_eq!(
            crate::proto::qsoripper::domain::Mode::Cw as i32,
            snapshot.mode
        );
        assert_eq!(None, snapshot.submode);
    }

    #[tokio::test]
    async fn connection_refused_returns_transport_error() {
        let provider = RigctldProvider::new(RigctldConfig {
            host: "127.0.0.1".to_string(),
            port: 1, // unlikely to be open
            read_timeout: Duration::from_millis(500),
        });

        let error = provider.get_snapshot().await.expect_err("error");

        assert!(error.is_retryable());
    }

    #[tokio::test]
    async fn rigctld_error_response_returns_parse_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);

            let mut cmd = String::new();
            reader.read_line(&mut cmd).await.unwrap();
            writer.write_all(b"RPRT -1\n").await.unwrap();
        });

        let provider = RigctldProvider::new(RigctldConfig {
            host: addr.ip().to_string(),
            port: addr.port(),
            read_timeout: Duration::from_secs(2),
        });

        let error = provider.get_snapshot().await.expect_err("error");

        assert_eq!(RigControlProviderErrorKind::Parse, error.kind());
        assert!(!error.is_retryable());
    }

    #[test]
    fn config_from_value_provider_enabled_by_default() {
        let config = RigctldConfig::from_value_provider(|_| None);
        let config = config.expect("enabled by default");
        assert_eq!("127.0.0.1", config.host);
        assert_eq!(4532, config.port);
    }

    #[test]
    fn config_from_value_provider_disabled_explicitly() {
        let config = RigctldConfig::from_value_provider(|name| match name {
            "QSORIPPER_RIGCTLD_ENABLED" => Some("false".to_string()),
            _ => None,
        });
        assert!(config.is_none());
    }

    #[test]
    fn config_from_value_provider_with_defaults() {
        let config = RigctldConfig::from_value_provider(|name| match name {
            "QSORIPPER_RIGCTLD_ENABLED" => Some("true".to_string()),
            _ => None,
        });

        let config = config.expect("config");
        assert_eq!("127.0.0.1", config.host);
        assert_eq!(4532, config.port);
        assert_eq!(Duration::from_millis(2000), config.read_timeout);
    }

    #[test]
    fn config_from_value_provider_with_overrides() {
        let config = RigctldConfig::from_value_provider(|name| match name {
            "QSORIPPER_RIGCTLD_ENABLED" => Some("1".to_string()),
            "QSORIPPER_RIGCTLD_HOST" => Some("192.168.1.100".to_string()),
            "QSORIPPER_RIGCTLD_PORT" => Some("4533".to_string()),
            "QSORIPPER_RIGCTLD_READ_TIMEOUT_MS" => Some("5000".to_string()),
            _ => None,
        });

        let config = config.expect("config");
        assert_eq!("192.168.1.100", config.host);
        assert_eq!(4533, config.port);
        assert_eq!(Duration::from_millis(5000), config.read_timeout);
    }
}
