//! QSO entry form state and field navigation.

use chrono::Utc;

/// Band names in display order, used as ADIF band strings.
pub(crate) const BANDS: &[&str] = &[
    "160M", "80M", "60M", "40M", "30M", "20M", "17M", "15M", "12M", "10M", "6M", "2M", "70CM",
];

/// Mode names in display order, used as ADIF mode strings.
pub(crate) const MODES: &[&str] = &["SSB", "CW", "FT8", "FT4", "RTTY", "PSK31", "AM", "FM"];

/// Default center frequency in MHz for each entry in [`BANDS`], in the same order.
pub(crate) const BAND_DEFAULT_FREQS: &[f64] = &[
    1.900, 3.750, 5.330, 7.150, 10.125, 14.225, 18.100, 21.200, 24.940, 28.400, 50.125, 146.520,
    446.000,
];

/// Index of 20M in [`BANDS`] — used as the startup default.
const DEFAULT_BAND_IDX: usize = 5;

/// Tabs available in the Advanced view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdvancedTab {
    Main,
    Contest,
    Technical,
    Awards,
}

impl AdvancedTab {
    pub(crate) const ALL: &'static [AdvancedTab] = &[
        AdvancedTab::Main,
        AdvancedTab::Contest,
        AdvancedTab::Technical,
        AdvancedTab::Awards,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            AdvancedTab::Main => "Main",
            AdvancedTab::Contest => "Contest",
            AdvancedTab::Technical => "Technical",
            AdvancedTab::Awards => "Awards",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            AdvancedTab::Main => AdvancedTab::Contest,
            AdvancedTab::Contest => AdvancedTab::Technical,
            AdvancedTab::Technical => AdvancedTab::Awards,
            AdvancedTab::Awards => AdvancedTab::Main,
        }
    }

    pub(crate) fn prev(self) -> Self {
        match self {
            AdvancedTab::Main => AdvancedTab::Awards,
            AdvancedTab::Contest => AdvancedTab::Main,
            AdvancedTab::Technical => AdvancedTab::Contest,
            AdvancedTab::Awards => AdvancedTab::Technical,
        }
    }
}

/// Focusable fields in the QSO entry form.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Field {
    /// Worked callsign.
    Callsign,
    /// Band selector (cycles through [`BANDS`]).
    Band,
    /// Mode selector (cycles through [`MODES`]).
    Mode,
    /// RST sent report.
    RstSent,
    /// RST received report.
    RstRcvd,
    /// Short comment.
    Comment,
    /// Operator notes.
    Notes,
    /// Frequency in MHz (free-text).
    FrequencyMhz,
    /// UTC date (`YYYY-MM-DD`).
    Date,
    /// UTC time on / start (`HH:MM`).
    Time,
    /// UTC time off / end (`HH:MM`).
    TimeOff,
    /// Worked station QTH (city/location).
    Qth,
    // Advanced fields shown in the advanced view.
    /// Transmitter power.
    TxPower,
    /// Submode override (e.g., USB, LSB, FT4).
    Submode,
    /// Contest identifier.
    ContestId,
    /// Serial number sent.
    SerialSent,
    /// Serial number received.
    SerialRcvd,
    /// Exchange sent.
    ExchangeSent,
    /// Exchange received.
    ExchangeRcvd,
    /// Propagation mode (ADIF `PROP_MODE`).
    PropMode,
    /// Satellite name.
    SatName,
    /// Satellite mode.
    SatMode,
    /// IOTA designator.
    Iota,
    /// ARRL section.
    ArrlSection,
    /// Worked state (US state abbreviation).
    WorkedState,
    /// Worked county.
    WorkedCounty,
    /// Worked operator name.
    WorkedName,
}

/// Primary navigation order for Tab/Shift-Tab in the log entry view.
const FIELD_ORDER: &[Field] = &[
    Field::Callsign,
    Field::Band,
    Field::Mode,
    Field::RstSent,
    Field::RstRcvd,
    Field::Comment,
    Field::Notes,
    Field::FrequencyMhz,
    Field::Date,
    Field::Time,
    Field::TimeOff,
    Field::Qth,
];

/// Fields for the Advanced "Main" tab — mirrors the basic form.
const ADV_MAIN_FIELDS: &[Field] = &[
    Field::Callsign,
    Field::Band,
    Field::Mode,
    Field::FrequencyMhz,
    Field::Date,
    Field::Time,
    Field::TimeOff,
    Field::Qth,
    Field::WorkedName,
    Field::RstSent,
    Field::RstRcvd,
    Field::Comment,
    Field::Notes,
];

/// Fields for the Advanced "Contest" tab.
const ADV_CONTEST_FIELDS: &[Field] = &[
    Field::TxPower,
    Field::Submode,
    Field::ContestId,
    Field::SerialSent,
    Field::SerialRcvd,
    Field::ExchangeSent,
    Field::ExchangeRcvd,
];

/// Fields for the Advanced "Technical" tab.
const ADV_TECHNICAL_FIELDS: &[Field] = &[Field::PropMode, Field::SatName, Field::SatMode];

/// Fields for the Advanced "Awards" tab.
const ADV_AWARDS_FIELDS: &[Field] = &[
    Field::Iota,
    Field::ArrlSection,
    Field::WorkedState,
    Field::WorkedCounty,
];

/// State of the QSO entry form (basic + advanced fields).
#[derive(Clone)]
pub(crate) struct LogForm {
    /// Currently focused field.
    pub(crate) focused: Field,
    /// When `true`, the focused field's text is fully selected; typing replaces it.
    pub(crate) field_selected: bool,
    /// Active tab in the Advanced view.
    pub(crate) advanced_tab: AdvancedTab,
    /// Worked callsign text.
    pub(crate) callsign: String,
    /// Index into [`BANDS`].
    pub(crate) band_idx: usize,
    /// Index into [`MODES`].
    pub(crate) mode_idx: usize,
    /// Frequency in MHz as a display string.
    pub(crate) frequency_mhz: String,
    /// Date in `YYYY-MM-DD` format.
    pub(crate) date: String,
    /// Time on (start) in `HH:MM` format.
    pub(crate) time: String,
    /// Time off (end) in `HH:MM` format; empty means same as time on.
    pub(crate) time_off: String,
    /// Worked station QTH (city/location).
    pub(crate) qth: String,
    /// RST sent report string.
    pub(crate) rst_sent: String,
    /// RST received report string.
    pub(crate) rst_rcvd: String,
    /// Short comment.
    pub(crate) comment: String,
    /// Operator notes.
    pub(crate) notes: String,
    // Advanced — Contest tab
    /// Transmitter power (e.g., "100W", "5W").
    pub(crate) tx_power: String,
    /// Submode override supplied by operator (overrides mode-derived submode).
    pub(crate) submode_override: String,
    /// Contest identifier (e.g., "CQWW", "ARRL-DX").
    pub(crate) contest_id: String,
    /// Contest serial number sent.
    pub(crate) serial_sent: String,
    /// Contest serial number received.
    pub(crate) serial_rcvd: String,
    /// Full exchange sent string.
    pub(crate) exchange_sent: String,
    /// Full exchange received string.
    pub(crate) exchange_rcvd: String,
    // Advanced — Technical tab
    /// Propagation mode (ADIF `PROP_MODE` value, e.g., "ES", "TEP", "SAT").
    pub(crate) prop_mode: String,
    /// Satellite name (e.g., "AO-7").
    pub(crate) sat_name: String,
    /// Satellite mode (e.g., "V/U").
    pub(crate) sat_mode: String,
    // Advanced — Awards tab
    /// IOTA designator (e.g., "EU-005").
    pub(crate) iota: String,
    /// ARRL section abbreviation (e.g., "WWA", "ENY").
    pub(crate) arrl_section: String,
    /// Worked US state abbreviation.
    pub(crate) worked_state: String,
    /// Worked county name.
    pub(crate) worked_county: String,
    /// Worked operator name (from lookup or manual entry).
    pub(crate) worked_name: String,
}

impl Default for LogForm {
    fn default() -> Self {
        Self::new()
    }
}

impl LogForm {
    /// Create a new form initialised with current UTC date/time and 20M/SSB defaults.
    pub(crate) fn new() -> Self {
        let now = Utc::now();
        let mut form = Self {
            focused: Field::Callsign,
            field_selected: false,
            advanced_tab: AdvancedTab::Main,
            callsign: String::new(),
            band_idx: DEFAULT_BAND_IDX,
            mode_idx: 0,
            frequency_mhz: String::new(),
            date: now.format("%Y-%m-%d").to_string(),
            time: now.format("%H:%M").to_string(),
            time_off: String::new(),
            qth: String::new(),
            rst_sent: String::new(),
            rst_rcvd: String::new(),
            comment: String::new(),
            notes: String::new(),
            tx_power: String::new(),
            submode_override: String::new(),
            contest_id: String::new(),
            serial_sent: String::new(),
            serial_rcvd: String::new(),
            exchange_sent: String::new(),
            exchange_rcvd: String::new(),
            prop_mode: String::new(),
            sat_name: String::new(),
            sat_mode: String::new(),
            iota: String::new(),
            arrl_section: String::new(),
            worked_state: String::new(),
            worked_county: String::new(),
            worked_name: String::new(),
        };
        form.on_band_change();
        form
    }

    /// Move focus to the next basic field, wrapping around, and select its text.
    pub(crate) fn next_field(&mut self) {
        let idx = FIELD_ORDER
            .iter()
            .position(|f| f == &self.focused)
            .unwrap_or(0);
        self.focused = FIELD_ORDER
            .get((idx + 1) % FIELD_ORDER.len())
            .cloned()
            .unwrap_or(Field::Callsign);
        self.field_selected = true;
    }

    /// Move focus to the previous basic field, wrapping around, and select its text.
    pub(crate) fn prev_field(&mut self) {
        let idx = FIELD_ORDER
            .iter()
            .position(|f| f == &self.focused)
            .unwrap_or(0);
        let new_idx = if idx == 0 {
            FIELD_ORDER.len().saturating_sub(1)
        } else {
            idx - 1
        };
        self.focused = FIELD_ORDER.get(new_idx).cloned().unwrap_or(Field::Callsign);
        self.field_selected = true;
    }

    /// Return the field list for the current advanced tab.
    pub(crate) fn current_advanced_fields(&self) -> &'static [Field] {
        match self.advanced_tab {
            AdvancedTab::Main => ADV_MAIN_FIELDS,
            AdvancedTab::Contest => ADV_CONTEST_FIELDS,
            AdvancedTab::Technical => ADV_TECHNICAL_FIELDS,
            AdvancedTab::Awards => ADV_AWARDS_FIELDS,
        }
    }

    /// Move focus to the next field in the current advanced tab, and select its text.
    pub(crate) fn next_advanced_field(&mut self) {
        let fields = self.current_advanced_fields();
        let idx = fields.iter().position(|f| f == &self.focused).unwrap_or(0);
        self.focused = fields
            .get((idx + 1) % fields.len())
            .cloned()
            .unwrap_or(Field::Callsign);
        self.field_selected = true;
    }

    /// Move focus to the previous field in the current advanced tab, and select its text.
    pub(crate) fn prev_advanced_field(&mut self) {
        let fields = self.current_advanced_fields();
        let idx = fields.iter().position(|f| f == &self.focused).unwrap_or(0);
        let new_idx = if idx == 0 {
            fields.len().saturating_sub(1)
        } else {
            idx - 1
        };
        self.focused = fields.get(new_idx).cloned().unwrap_or(Field::Callsign);
        self.field_selected = true;
    }

    /// Switch to the next advanced tab and focus its first field.
    pub(crate) fn next_advanced_tab(&mut self) {
        self.advanced_tab = self.advanced_tab.next();
        self.focused = self
            .current_advanced_fields()
            .first()
            .cloned()
            .unwrap_or(Field::Callsign);
        self.field_selected = true;
    }

    /// Switch to the previous advanced tab and focus its first field.
    pub(crate) fn prev_advanced_tab(&mut self) {
        self.advanced_tab = self.advanced_tab.prev();
        self.focused = self
            .current_advanced_fields()
            .first()
            .cloned()
            .unwrap_or(Field::Callsign);
        self.field_selected = true;
    }

    /// Update frequency and RST defaults after the band changes.
    pub(crate) fn on_band_change(&mut self) {
        let freq = BAND_DEFAULT_FREQS
            .get(self.band_idx)
            .copied()
            .unwrap_or(14.225);
        self.frequency_mhz = format!("{freq:.3}");
        self.on_mode_change();
    }

    /// Update RST defaults to match the currently selected mode.
    pub(crate) fn on_mode_change(&mut self) {
        let rst = default_rst_for_mode(self.mode_idx);
        self.rst_sent = rst.to_string();
        self.rst_rcvd = rst.to_string();
    }

    /// Return a mutable reference to the focused field's text buffer.
    ///
    /// Returns `None` for cycle-only fields (`Band`, `Mode`).
    pub(crate) fn current_field_text_mut(&mut self) -> Option<&mut String> {
        match self.focused {
            Field::Callsign => Some(&mut self.callsign),
            Field::FrequencyMhz => Some(&mut self.frequency_mhz),
            Field::Date => Some(&mut self.date),
            Field::Time => Some(&mut self.time),
            Field::TimeOff => Some(&mut self.time_off),
            Field::Qth => Some(&mut self.qth),
            Field::RstSent => Some(&mut self.rst_sent),
            Field::RstRcvd => Some(&mut self.rst_rcvd),
            Field::Comment => Some(&mut self.comment),
            Field::Notes => Some(&mut self.notes),
            Field::TxPower => Some(&mut self.tx_power),
            Field::Submode => Some(&mut self.submode_override),
            Field::ContestId => Some(&mut self.contest_id),
            Field::SerialSent => Some(&mut self.serial_sent),
            Field::SerialRcvd => Some(&mut self.serial_rcvd),
            Field::ExchangeSent => Some(&mut self.exchange_sent),
            Field::ExchangeRcvd => Some(&mut self.exchange_rcvd),
            Field::PropMode => Some(&mut self.prop_mode),
            Field::SatName => Some(&mut self.sat_name),
            Field::SatMode => Some(&mut self.sat_mode),
            Field::Iota => Some(&mut self.iota),
            Field::ArrlSection => Some(&mut self.arrl_section),
            Field::WorkedState => Some(&mut self.worked_state),
            Field::WorkedCounty => Some(&mut self.worked_county),
            Field::WorkedName => Some(&mut self.worked_name),
            Field::Band | Field::Mode => None,
        }
    }

    /// Current band name from the [`BANDS`] slice.
    pub(crate) fn band_str(&self) -> &str {
        BANDS.get(self.band_idx).copied().unwrap_or("20M")
    }

    /// Current mode name from the [`MODES`] slice.
    pub(crate) fn mode_str(&self) -> &str {
        MODES.get(self.mode_idx).copied().unwrap_or("SSB")
    }

    /// Returns `true` if the focused field is a Left/Right cycle selector.
    pub(crate) fn is_cycle_field(&self) -> bool {
        matches!(self.focused, Field::Band | Field::Mode)
    }

    /// Cycle to the next band whose name starts with `ch` (case-insensitive).
    ///
    /// Repeated calls with the same char advance through all matching bands, wrapping around.
    pub(crate) fn type_select_band(&mut self, ch: char) {
        let ch_lo = ch.to_ascii_lowercase();
        let matches: Vec<usize> = BANDS
            .iter()
            .enumerate()
            .filter(|(_, b)| b.chars().next().map(|c| c.to_ascii_lowercase()) == Some(ch_lo))
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return;
        }
        let pos = matches.iter().position(|&i| i == self.band_idx);
        let next = pos.map_or(0, |p| (p + 1) % matches.len());
        if let Some(&idx) = matches.get(next) {
            self.band_idx = idx;
            self.on_band_change();
        }
    }

    /// Cycle to the next mode whose name starts with `ch` (case-insensitive).
    ///
    /// Repeated calls with the same char advance through all matching modes, wrapping around.
    pub(crate) fn type_select_mode(&mut self, ch: char) {
        let ch_lo = ch.to_ascii_lowercase();
        let matches: Vec<usize> = MODES
            .iter()
            .enumerate()
            .filter(|(_, m)| m.chars().next().map(|c| c.to_ascii_lowercase()) == Some(ch_lo))
            .map(|(i, _)| i)
            .collect();
        if matches.is_empty() {
            return;
        }
        let pos = matches.iter().position(|&i| i == self.mode_idx);
        let next = pos.map_or(0, |p| (p + 1) % matches.len());
        if let Some(&idx) = matches.get(next) {
            self.mode_idx = idx;
            self.on_mode_change();
        }
    }
}

/// Return the default RST string for the given mode index.
fn default_rst_for_mode(mode_idx: usize) -> &'static str {
    match MODES.get(mode_idx).copied().unwrap_or("SSB") {
        "SSB" | "AM" | "FM" => "59",
        _ => "599",
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::type_complexity,
    clippy::items_after_statements
)]
mod tests {
    use super::*;

    #[test]
    fn new_form_starts_on_20m_ssb() {
        let form = LogForm::new();
        assert_eq!(form.band_str(), "20M");
        assert_eq!(form.mode_str(), "SSB");
        assert_eq!(form.focused, Field::Callsign);
        assert!(!form.field_selected);
    }

    #[test]
    fn new_form_sets_default_frequency() {
        let form = LogForm::new();
        assert_eq!(form.frequency_mhz, "14.225");
    }

    #[test]
    fn new_form_sets_rst_defaults_for_ssb() {
        let form = LogForm::new();
        assert_eq!(form.rst_sent, "59");
        assert_eq!(form.rst_rcvd, "59");
    }

    #[test]
    fn next_field_advances_focus() {
        let mut form = LogForm::new();
        assert_eq!(form.focused, Field::Callsign);
        form.next_field();
        assert_eq!(form.focused, Field::Band);
        assert!(form.field_selected);
    }

    #[test]
    fn next_field_wraps_to_start() {
        let mut form = LogForm::new();
        for _ in 0..FIELD_ORDER.len() {
            form.next_field();
        }
        assert_eq!(form.focused, Field::Callsign);
    }

    #[test]
    fn prev_field_moves_to_last_from_first() {
        let mut form = LogForm::new();
        form.prev_field();
        assert_eq!(form.focused, *FIELD_ORDER.last().unwrap());
        assert!(form.field_selected);
    }

    #[test]
    fn prev_field_from_second_returns_to_first() {
        let mut form = LogForm::new();
        form.next_field();
        form.prev_field();
        assert_eq!(form.focused, Field::Callsign);
    }

    #[test]
    fn on_mode_change_sets_cw_rst() {
        let mut form = LogForm::new();
        form.mode_idx = MODES.iter().position(|&m| m == "CW").unwrap();
        form.on_mode_change();
        assert_eq!(form.rst_sent, "599");
        assert_eq!(form.rst_rcvd, "599");
    }

    #[test]
    fn on_mode_change_sets_ft8_rst() {
        let mut form = LogForm::new();
        form.mode_idx = MODES.iter().position(|&m| m == "FT8").unwrap();
        form.on_mode_change();
        assert_eq!(form.rst_sent, "599");
        assert_eq!(form.rst_rcvd, "599");
    }

    #[test]
    fn on_mode_change_sets_ft4_rst() {
        let mut form = LogForm::new();
        form.mode_idx = MODES.iter().position(|&m| m == "FT4").unwrap();
        form.on_mode_change();
        assert_eq!(form.rst_sent, "599");
    }

    #[test]
    fn on_mode_change_sets_am_rst() {
        let mut form = LogForm::new();
        form.mode_idx = MODES.iter().position(|&m| m == "AM").unwrap();
        form.on_mode_change();
        assert_eq!(form.rst_sent, "59");
    }

    #[test]
    fn on_mode_change_sets_fm_rst() {
        let mut form = LogForm::new();
        form.mode_idx = MODES.iter().position(|&m| m == "FM").unwrap();
        form.on_mode_change();
        assert_eq!(form.rst_sent, "59");
    }

    #[test]
    fn on_mode_change_out_of_bounds_defaults_to_ssb_rst() {
        let mut form = LogForm::new();
        form.mode_idx = 999;
        form.on_mode_change();
        assert_eq!(form.rst_sent, "59");
    }

    #[test]
    fn on_band_change_updates_frequency_for_40m() {
        let mut form = LogForm::new();
        form.band_idx = BANDS.iter().position(|&b| b == "40M").unwrap();
        form.on_band_change();
        assert_eq!(form.frequency_mhz, "7.150");
    }

    #[test]
    fn on_band_change_updates_frequency_for_160m() {
        let mut form = LogForm::new();
        form.band_idx = 0;
        form.on_band_change();
        assert_eq!(form.frequency_mhz, "1.900");
    }

    #[test]
    fn on_band_change_out_of_bounds_uses_fallback() {
        let mut form = LogForm::new();
        form.band_idx = 999;
        form.on_band_change();
        assert_eq!(form.frequency_mhz, "14.225");
    }

    #[test]
    fn band_str_out_of_bounds_returns_20m() {
        let mut form = LogForm::new();
        form.band_idx = 999;
        assert_eq!(form.band_str(), "20M");
    }

    #[test]
    fn mode_str_out_of_bounds_returns_ssb() {
        let mut form = LogForm::new();
        form.mode_idx = 999;
        assert_eq!(form.mode_str(), "SSB");
    }

    #[test]
    fn is_cycle_field_true_for_band_and_mode() {
        let mut form = LogForm::new();
        form.focused = Field::Band;
        assert!(form.is_cycle_field());
        form.focused = Field::Mode;
        assert!(form.is_cycle_field());
    }

    #[test]
    fn is_cycle_field_false_for_callsign() {
        let form = LogForm::new();
        assert!(!form.is_cycle_field());
    }

    #[test]
    fn current_field_text_mut_callsign() {
        let mut form = LogForm::new();
        form.focused = Field::Callsign;
        {
            let text = form.current_field_text_mut().unwrap();
            text.push_str("K7ABC");
        }
        assert_eq!(form.callsign, "K7ABC");
    }

    #[test]
    fn current_field_text_mut_all_text_fields() {
        let fields_and_setters: Vec<(Field, Box<dyn Fn(&LogForm) -> &str>)> = vec![
            (
                Field::FrequencyMhz,
                Box::new(|f: &LogForm| f.frequency_mhz.as_str()),
            ),
            (Field::Date, Box::new(|f: &LogForm| f.date.as_str())),
            (Field::Time, Box::new(|f: &LogForm| f.time.as_str())),
            (Field::TimeOff, Box::new(|f: &LogForm| f.time_off.as_str())),
            (Field::Qth, Box::new(|f: &LogForm| f.qth.as_str())),
            (Field::RstSent, Box::new(|f: &LogForm| f.rst_sent.as_str())),
            (Field::RstRcvd, Box::new(|f: &LogForm| f.rst_rcvd.as_str())),
            (Field::Comment, Box::new(|f: &LogForm| f.comment.as_str())),
            (Field::Notes, Box::new(|f: &LogForm| f.notes.as_str())),
            (Field::TxPower, Box::new(|f: &LogForm| f.tx_power.as_str())),
            (
                Field::Submode,
                Box::new(|f: &LogForm| f.submode_override.as_str()),
            ),
            (
                Field::ContestId,
                Box::new(|f: &LogForm| f.contest_id.as_str()),
            ),
            (
                Field::SerialSent,
                Box::new(|f: &LogForm| f.serial_sent.as_str()),
            ),
            (
                Field::SerialRcvd,
                Box::new(|f: &LogForm| f.serial_rcvd.as_str()),
            ),
            (
                Field::ExchangeSent,
                Box::new(|f: &LogForm| f.exchange_sent.as_str()),
            ),
            (
                Field::ExchangeRcvd,
                Box::new(|f: &LogForm| f.exchange_rcvd.as_str()),
            ),
            (
                Field::PropMode,
                Box::new(|f: &LogForm| f.prop_mode.as_str()),
            ),
            (Field::SatName, Box::new(|f: &LogForm| f.sat_name.as_str())),
            (Field::SatMode, Box::new(|f: &LogForm| f.sat_mode.as_str())),
            (Field::Iota, Box::new(|f: &LogForm| f.iota.as_str())),
            (
                Field::ArrlSection,
                Box::new(|f: &LogForm| f.arrl_section.as_str()),
            ),
            (
                Field::WorkedState,
                Box::new(|f: &LogForm| f.worked_state.as_str()),
            ),
            (
                Field::WorkedCounty,
                Box::new(|f: &LogForm| f.worked_county.as_str()),
            ),
            (
                Field::WorkedName,
                Box::new(|f: &LogForm| f.worked_name.as_str()),
            ),
        ];
        for (field, _getter) in &fields_and_setters {
            let mut form = LogForm::new();
            form.focused = field.clone();
            assert!(
                form.current_field_text_mut().is_some(),
                "Field {field:?} should return Some"
            );
        }
    }

    #[test]
    fn current_field_text_mut_none_for_cycle_fields() {
        let mut form = LogForm::new();
        form.focused = Field::Band;
        assert!(form.current_field_text_mut().is_none());
        form.focused = Field::Mode;
        assert!(form.current_field_text_mut().is_none());
    }

    #[test]
    fn type_select_band_cycles_through_matching() {
        let mut form = LogForm::new();
        form.band_idx = BANDS.iter().position(|&b| b == "160M").unwrap();
        form.type_select_band('1');
        assert_eq!(form.band_str(), "17M");
        form.type_select_band('1');
        assert_eq!(form.band_str(), "15M");
        form.type_select_band('1');
        assert_eq!(form.band_str(), "12M");
        form.type_select_band('1');
        assert_eq!(form.band_str(), "10M");
        form.type_select_band('1');
        assert_eq!(form.band_str(), "160M");
    }

    #[test]
    fn type_select_band_ignores_unknown_char() {
        let mut form = LogForm::new();
        let original = form.band_idx;
        form.type_select_band('z');
        assert_eq!(form.band_idx, original);
    }

    #[test]
    fn type_select_mode_selects_by_first_char() {
        let mut form = LogForm::new();
        form.type_select_mode('c');
        assert_eq!(form.mode_str(), "CW");
    }

    #[test]
    fn type_select_mode_cycles_through_matching() {
        let mut form = LogForm::new();
        form.type_select_mode('f');
        let first = form.mode_str().to_string();
        form.type_select_mode('f');
        let second = form.mode_str().to_string();
        assert_ne!(first, second);
    }

    #[test]
    fn type_select_mode_ignores_unknown_char() {
        let mut form = LogForm::new();
        let original = form.mode_idx;
        form.type_select_mode('z');
        assert_eq!(form.mode_idx, original);
    }

    #[test]
    fn advanced_tab_labels() {
        assert_eq!(AdvancedTab::Main.label(), "Main");
        assert_eq!(AdvancedTab::Contest.label(), "Contest");
        assert_eq!(AdvancedTab::Technical.label(), "Technical");
        assert_eq!(AdvancedTab::Awards.label(), "Awards");
    }

    #[test]
    fn advanced_tab_next_cycles_all() {
        assert_eq!(AdvancedTab::Main.next(), AdvancedTab::Contest);
        assert_eq!(AdvancedTab::Contest.next(), AdvancedTab::Technical);
        assert_eq!(AdvancedTab::Technical.next(), AdvancedTab::Awards);
        assert_eq!(AdvancedTab::Awards.next(), AdvancedTab::Main);
    }

    #[test]
    fn advanced_tab_prev_cycles_all() {
        assert_eq!(AdvancedTab::Main.prev(), AdvancedTab::Awards);
        assert_eq!(AdvancedTab::Awards.prev(), AdvancedTab::Technical);
        assert_eq!(AdvancedTab::Technical.prev(), AdvancedTab::Contest);
        assert_eq!(AdvancedTab::Contest.prev(), AdvancedTab::Main);
    }

    #[test]
    fn all_advanced_tabs_count() {
        assert_eq!(AdvancedTab::ALL.len(), 4);
    }

    #[test]
    fn next_advanced_tab_updates_focus() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Main;
        form.next_advanced_tab();
        assert_eq!(form.advanced_tab, AdvancedTab::Contest);
        assert!(form.field_selected);
    }

    #[test]
    fn prev_advanced_tab_updates_focus() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Main;
        form.prev_advanced_tab();
        assert_eq!(form.advanced_tab, AdvancedTab::Awards);
        assert!(form.field_selected);
    }

    #[test]
    fn next_advanced_field_wraps_in_contest_tab() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Contest;
        let count = ADV_CONTEST_FIELDS.len();
        form.focused = ADV_CONTEST_FIELDS[0].clone();
        for _ in 0..count {
            form.next_advanced_field();
        }
        assert_eq!(form.focused, ADV_CONTEST_FIELDS[0]);
    }

    #[test]
    fn prev_advanced_field_from_first_wraps_to_last() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Technical;
        let fields = form.current_advanced_fields();
        form.focused = fields[0].clone();
        form.prev_advanced_field();
        assert_eq!(form.focused, *fields.last().unwrap());
    }

    #[test]
    fn current_advanced_fields_main_tab() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Main;
        let fields = form.current_advanced_fields();
        assert!(fields.contains(&Field::Callsign));
        assert!(fields.contains(&Field::WorkedName));
    }

    #[test]
    fn current_advanced_fields_contest_tab() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Contest;
        let fields = form.current_advanced_fields();
        assert!(fields.contains(&Field::ContestId));
        assert!(fields.contains(&Field::SerialSent));
    }

    #[test]
    fn current_advanced_fields_technical_tab() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Technical;
        let fields = form.current_advanced_fields();
        assert!(fields.contains(&Field::PropMode));
        assert!(fields.contains(&Field::SatName));
    }

    #[test]
    fn current_advanced_fields_awards_tab() {
        let mut form = LogForm::new();
        form.advanced_tab = AdvancedTab::Awards;
        let fields = form.current_advanced_fields();
        assert!(fields.contains(&Field::Iota));
        assert!(fields.contains(&Field::ArrlSection));
    }
}
