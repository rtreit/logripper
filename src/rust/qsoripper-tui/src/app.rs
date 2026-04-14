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
    /// Confirmation dialog before deleting a QSO.
    ConfirmDeleteQso,
}

/// Resolved callsign information from a QRZ lookup (used for field auto-population).
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
    pub(crate) dxcc: Option<u32>,
}

/// Display-ready row for the recent QSOs list.
pub(crate) struct RecentQso {
    /// QsoRipper-assigned local UUID.
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
    /// Worked operator name.
    pub(crate) name: Option<String>,
}

impl RecentQso {
    /// Returns `true` if any visible column contains `lower` (case-insensitive, already lowercased).
    pub(crate) fn matches_search(&self, lower: &str) -> bool {
        self.callsign.to_lowercase().contains(lower)
            || self.band.to_lowercase().contains(lower)
            || self.mode.to_lowercase().contains(lower)
            || self.rst_sent.to_lowercase().contains(lower)
            || self.rst_rcvd.to_lowercase().contains(lower)
            || self
                .country
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(lower)
            || self
                .grid
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(lower)
            || self
                .name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains(lower)
            || self.utc.contains(lower)
    }
}

/// Current space weather conditions.
pub(crate) struct SpaceWeatherInfo {
    /// Planetary K-index (0–9).
    pub(crate) k_index: Option<f64>,
    /// Solar flux index (sfu).
    pub(crate) solar_flux: Option<f64>,
    /// International sunspot number.
    pub(crate) sunspot_number: Option<u32>,
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
#[expect(
    clippy::struct_excessive_bools,
    reason = "each bool is a distinct independent flag; a state machine would be more complex"
)]
pub(crate) struct App {
    /// Active view / screen.
    pub(crate) view: View,
    /// QSO entry form state.
    pub(crate) form: LogForm,
    /// Current UTC date+time string for the clock display (`YYYY-MM-DD HH:MM:SS`).
    pub(crate) utc_now: String,
    /// Most recent callsign lookup result (used for field auto-population; not displayed).
    pub(crate) lookup_result: Option<CallsignInfo>,
    /// All QSOs loaded from the engine (unfiltered source of truth).
    pub(crate) recent_qsos: Vec<RecentQso>,
    /// Current space weather snapshot.
    pub(crate) space_weather: Option<SpaceWeatherInfo>,
    /// Transient status bar message.
    pub(crate) status_message: Option<StatusMessage>,
    /// Whether keyboard focus is on the recent QSOs list panel.
    pub(crate) qso_list_focused: bool,
    /// Selected row index within the currently filtered QSO list (when list is focused).
    pub(crate) qso_selected: Option<usize>,
    /// Local ID of the QSO currently being edited (`None` means a new QSO is being entered).
    pub(crate) editing_local_id: Option<String>,
    /// Local ID of the QSO pending deletion (set when `ConfirmDeleteQso` view is active).
    pub(crate) delete_candidate_id: Option<String>,
    /// Search / filter text for the QSO list.
    pub(crate) search_text: String,
    /// Whether keyboard focus is on the search box.
    pub(crate) search_focused: bool,
    /// Whether the main event loop should keep running.
    pub(crate) running: bool,
    /// gRPC server endpoint URL.
    pub(crate) endpoint: String,
    /// When the current QSO started — used for the live duration display.
    ///
    /// Reset by F7, by Esc (form clear), and after each QSO is successfully logged or updated.
    pub(crate) qso_started_at: Instant,
    /// Set when the debounce fires and the duration timer is actively counting.
    ///
    /// Starts `false`; becomes `true` once a callsign has been stable for ~1.5 s.
    /// Cleared on form reset, Esc, and after log/update.
    pub(crate) qso_timer_active: bool,
    /// When the callsign field was last modified; drives the debounce that starts the timer.
    pub(crate) callsign_last_typed_at: Option<Instant>,
}

impl App {
    /// Create a new `App` targeting the given gRPC endpoint.
    pub(crate) fn new(endpoint: String) -> Self {
        let now = chrono::Utc::now();
        Self {
            view: View::LogEntry,
            form: LogForm::new(),
            utc_now: now.format("%Y-%m-%d %H:%M:%S").to_string(),
            lookup_result: None,
            recent_qsos: Vec::new(),
            space_weather: None,
            status_message: None,
            qso_list_focused: false,
            qso_selected: None,
            editing_local_id: None,
            delete_candidate_id: None,
            search_text: String::new(),
            search_focused: false,
            running: true,
            endpoint,
            qso_started_at: Instant::now(),
            qso_timer_active: false,
            callsign_last_typed_at: None,
        }
    }

    /// Returns references to the QSOs that match the current `search_text`.
    ///
    /// Returns all QSOs when `search_text` is empty.
    pub(crate) fn filtered_qsos(&self) -> Vec<&RecentQso> {
        if self.search_text.is_empty() {
            return self.recent_qsos.iter().collect();
        }
        let lower = self.search_text.to_lowercase();
        self.recent_qsos
            .iter()
            .filter(|q| q.matches_search(&lower))
            .collect()
    }

    /// Find a QSO in `recent_qsos` by its local ID.
    pub(crate) fn find_qso_by_id(&self, id: &str) -> Option<&RecentQso> {
        self.recent_qsos.iter().find(|q| q.local_id == id)
    }

    /// Reset the QSO start time to now and update the form date/time fields to match.
    pub(crate) fn reset_qso_start_time(&mut self) {
        let now = chrono::Utc::now();
        self.qso_started_at = Instant::now();
        self.qso_timer_active = true;
        self.form.date = now.format("%Y-%m-%d").to_string();
        self.form.time = now.format("%H:%M").to_string();
    }

    /// Called on each tick: activates the duration timer once the callsign has been
    /// stable (no typing) for at least 1.5 seconds.
    pub(crate) fn tick_debounce(&mut self) {
        if self.qso_timer_active || self.form.callsign.is_empty() {
            return;
        }
        if let Some(last) = self.callsign_last_typed_at {
            if last.elapsed().as_millis() >= 1500 {
                self.qso_started_at = Instant::now();
                self.qso_timer_active = true;
            }
        }
    }

    /// Reset all timer state — called after logging, updating, or clearing the form.
    pub(crate) fn reset_timer(&mut self) {
        self.qso_started_at = Instant::now();
        self.qso_timer_active = false;
        self.callsign_last_typed_at = None;
    }

    /// Signal that the callsign field was just modified.
    pub(crate) fn on_callsign_changed(&mut self) {
        self.callsign_last_typed_at = Some(Instant::now());
        if self.form.callsign.is_empty() {
            self.qso_timer_active = false;
        }
    }

    /// Format the elapsed QSO duration as `M:SS` (< 1 h) or `H:MM:SS`.
    ///
    /// Returns `None` when the timer is not yet active.
    pub(crate) fn qso_duration_str(&self) -> Option<String> {
        if !self.qso_timer_active {
            return None;
        }
        let secs = self.qso_started_at.elapsed().as_secs();
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        let s = secs % 60;
        Some(if h > 0 {
            format!("{h}:{m:02}:{s:02}")
        } else {
            format!("{m}:{s:02}")
        })
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
