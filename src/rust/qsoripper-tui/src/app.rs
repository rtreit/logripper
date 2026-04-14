//! Top-level application state shared across the main event loop and UI renderer.

use std::time::Instant;

use crate::form::LogForm;

/// Top-level view the TUI is currently showing.
pub(crate) enum View {
    /// QSO entry form — the primary screen.
    LogEntry,
    /// Extended field entry overlay (F2 to toggle).
    Advanced,
    /// Full-screen help overlay.
    Help,
}

/// Resolved callsign information shown in the lookup panel.
pub(crate) struct CallsignInfo {
    /// Queried callsign.
    pub(crate) callsign: String,
    /// Operator name (formatted).
    pub(crate) name: Option<String>,
    /// QTH / city.
    pub(crate) qth: Option<String>,
    /// Maidenhead grid square.
    pub(crate) grid: Option<String>,
    /// Country name.
    pub(crate) country: Option<String>,
    /// CQ zone number.
    pub(crate) cq_zone: Option<u32>,
    /// DXCC entity ID.
    #[expect(dead_code, reason = "reserved for future DX scoring display")]
    pub(crate) dxcc: Option<u32>,
}

/// Display-ready row for the recent QSOs list.
pub(crate) struct RecentQso {
    /// QsoRipper-assigned local UUID.
    #[expect(dead_code, reason = "reserved for future QSO edit/delete actions")]
    pub(crate) local_id: String,
    /// UTC time formatted as `HH:MM`.
    pub(crate) utc: String,
    /// Worked callsign.
    pub(crate) callsign: String,
    /// Band name string.
    pub(crate) band: String,
    /// Mode name string.
    pub(crate) mode: String,
    /// RST sent report string.
    pub(crate) rst_sent: String,
    /// RST received report string.
    pub(crate) rst_rcvd: String,
    /// Worked entity country.
    pub(crate) country: Option<String>,
    /// Worked grid square.
    pub(crate) grid: Option<String>,
}

/// Current space weather conditions.
pub(crate) struct SpaceWeatherInfo {
    /// Planetary K-index (0–9).
    pub(crate) k_index: Option<f64>,
    /// Solar flux index (sfu).
    pub(crate) solar_flux: Option<f64>,
    /// International sunspot number.
    pub(crate) sunspot_number: Option<u32>,
    /// Human-readable data-age status.
    pub(crate) status: String,
}

/// A transient status bar message that auto-expires after three seconds.
pub(crate) struct StatusMessage {
    /// Message text.
    pub(crate) text: String,
    /// `true` for error styling; `false` for success styling.
    pub(crate) is_error: bool,
    /// When the message was created, used to compute expiry.
    pub(crate) created_at: Instant,
}

/// Top-level application state passed through the event loop and renderer.
pub(crate) struct App {
    /// Active view / screen.
    pub(crate) view: View,
    /// QSO entry form state.
    pub(crate) form: LogForm,
    /// Most recent callsign lookup result.
    pub(crate) lookup_result: Option<CallsignInfo>,
    /// Recent QSOs for the history panel.
    pub(crate) recent_qsos: Vec<RecentQso>,
    /// Current space weather snapshot.
    pub(crate) space_weather: Option<SpaceWeatherInfo>,
    /// Transient status bar message.
    pub(crate) status_message: Option<StatusMessage>,
    /// Whether the main event loop should keep running.
    pub(crate) running: bool,
    /// gRPC server endpoint URL.
    pub(crate) endpoint: String,
}

impl App {
    /// Create a new `App` targeting the given gRPC endpoint.
    pub(crate) fn new(endpoint: String) -> Self {
        Self {
            view: View::LogEntry,
            form: LogForm::new(),
            lookup_result: None,
            recent_qsos: Vec::new(),
            space_weather: None,
            status_message: None,
            running: true,
            endpoint,
        }
    }

    /// Set a success status message, replacing any current message.
    pub(crate) fn set_status(&mut self, text: impl Into<String>) {
        self.status_message = Some(StatusMessage {
            text: text.into(),
            is_error: false,
            created_at: Instant::now(),
        });
    }

    /// Set an error status message, replacing any current message.
    pub(crate) fn set_error(&mut self, text: impl Into<String>) {
        self.status_message = Some(StatusMessage {
            text: text.into(),
            is_error: true,
            created_at: Instant::now(),
        });
    }

    /// Clear the status message if it has been visible for more than 3 seconds.
    pub(crate) fn expire_status(&mut self) {
        if let Some(msg) = &self.status_message {
            if msg.created_at.elapsed().as_secs() >= 3 {
                self.status_message = None;
            }
        }
    }
}
