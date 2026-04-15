//! QSO entry form rendering.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;
use crate::form::Field;

/// Fixed display width for the callsign field.
const CALLSIGN_WIDTH: usize = 12;
/// Fixed display width for RST fields.
const RST_WIDTH: usize = 5;
/// Fixed display width for the frequency field.
const FREQ_WIDTH: usize = 9;
/// Fixed display width for the date field.
const DATE_WIDTH: usize = 10;
/// Fixed display width for the time field.
const TIME_WIDTH: usize = 8;

/// Render the QSO log entry form into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let title = if app.editing_local_id.is_some() {
        " Edit QSO "
    } else {
        " New QSO "
    };
    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height < 9 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // callsign
        Constraint::Length(1), // band / mode
        Constraint::Length(1), // rst sent / rst rcvd
        Constraint::Length(1), // comment
        Constraint::Length(1), // notes
        Constraint::Length(1), // freq / date / time
        Constraint::Length(1), // QSO duration
        Constraint::Fill(1),   // padding
        Constraint::Length(1), // action hints
    ])
    .split(inner);

    let cs_area = layout.first().copied().unwrap_or(inner);
    let band_mode_area = layout.get(1).copied().unwrap_or(inner);
    let rst_area = layout.get(2).copied().unwrap_or(inner);
    let comment_area = layout.get(3).copied().unwrap_or(inner);
    let notes_area = layout.get(4).copied().unwrap_or(inner);
    let freq_area = layout.get(5).copied().unwrap_or(inner);
    let duration_area = layout.get(6).copied().unwrap_or(inner);
    let hints_area = layout.get(8).copied().unwrap_or(inner);

    let form = &app.form;

    render_callsign_row(frame, cs_area, form);
    render_band_mode_row(frame, band_mode_area, form);
    render_rst_row(frame, rst_area, form);
    render_comment_row(frame, comment_area, form);
    render_notes_row(frame, notes_area, form);
    render_freq_row(frame, freq_area, form);
    render_duration_row(frame, duration_area, app);
    render_hints_row(frame, hints_area);
}

/// Render the callsign row.
fn render_callsign_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let cs_focused = form.focused == Field::Callsign;
    let cs_selected = cs_focused && form.field_selected;
    let cs_val = field_value(&form.callsign, cs_focused, cs_selected, CALLSIGN_WIDTH);
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(label_m("", 'C', "allsign "));
    spans.push(styled_field(cs_val, cs_focused, cs_selected));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the band / mode row.
fn render_band_mode_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let band_val = cycle_value(form.band_str(), form.focused == Field::Band);
    let mode_val = cycle_value(form.mode_str(), form.focused == Field::Mode);
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(label_m("", 'B', "and "));
    spans.push(styled_cycle(band_val, form.focused == Field::Band));
    spans.push(Span::raw("  "));
    spans.extend(label_m("", 'M', "ode "));
    spans.push(styled_cycle(mode_val, form.focused == Field::Mode));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the RST sent / RST received row.
fn render_rst_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let sent_focused = form.focused == Field::RstSent;
    let sent_selected = sent_focused && form.field_selected;
    let rcvd_focused = form.focused == Field::RstRcvd;
    let rcvd_selected = rcvd_focused && form.field_selected;
    let sent_val = field_value(&form.rst_sent, sent_focused, sent_selected, RST_WIDTH);
    let rcvd_val = field_value(&form.rst_rcvd, rcvd_focused, rcvd_selected, RST_WIDTH);
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(label_m("RST ", 'S', "nt  "));
    spans.push(styled_field(sent_val, sent_focused, sent_selected));
    spans.push(Span::raw("   "));
    spans.extend(label_m("RST ", 'R', "cvd "));
    spans.push(styled_field(rcvd_val, rcvd_focused, rcvd_selected));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the comment row.
fn render_comment_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let label_len: usize = 9;
    let width = (area.width as usize).saturating_sub(label_len + 2).max(10);
    let focused = form.focused == Field::Comment;
    let selected = focused && form.field_selected;
    let val = field_value(&form.comment, focused, selected, width);
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(label_m("C", 'o', "mment  "));
    spans.push(styled_field(val, focused, selected));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the notes row.
fn render_notes_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let label_len: usize = 9;
    let width = (area.width as usize).saturating_sub(label_len + 2).max(10);
    let focused = form.focused == Field::Notes;
    let selected = focused && form.field_selected;
    let val = field_value(&form.notes, focused, selected, width);
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(label_m("", 'N', "otes    "));
    spans.push(styled_field(val, focused, selected));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the frequency / date / time row.
fn render_freq_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let freq_focused = form.focused == Field::FrequencyMhz;
    let freq_selected = freq_focused && form.field_selected;
    let date_focused = form.focused == Field::Date;
    let date_selected = date_focused && form.field_selected;
    let time_focused = form.focused == Field::Time;
    let time_selected = time_focused && form.field_selected;
    let freq_val = field_value(&form.frequency_mhz, freq_focused, freq_selected, FREQ_WIDTH);
    let date_val = field_value(&form.date, date_focused, date_selected, DATE_WIDTH);
    let time_val = field_value(&form.time, time_focused, time_selected, TIME_WIDTH);
    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.extend(label_m("", 'F', "req MHz "));
    spans.push(styled_field(freq_val, freq_focused, freq_selected));
    spans.push(Span::raw("  "));
    spans.extend(label_m("", 'D', "ate "));
    spans.push(styled_field(date_val, date_focused, date_selected));
    spans.push(Span::raw("  "));
    spans.extend(label_m("", 'T', "ime "));
    spans.push(styled_field(time_val, time_focused, time_selected));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Render the live QSO duration row.
fn render_duration_row(frame: &mut Frame, area: Rect, app: &App) {
    let (label, style) = match app.qso_duration_str() {
        Some(d) => (
            d,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        None if app.form.callsign.is_empty() => {
            ("---".to_string(), Style::default().fg(Color::DarkGray))
        }
        None => (
            "starting\u{2026}".to_string(),
            Style::default().fg(Color::DarkGray),
        ),
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("QSO duration  ", Style::default().fg(Color::Cyan)),
            Span::styled(label, style),
        ])),
        area,
    );
}

/// Render the action keyboard-hints row.
fn render_hints_row(frame: &mut Frame, area: Rect) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                " F10 Log QSO ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(
                " Esc Clear ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::raw("  "),
            Span::styled(
                " F2 Advanced ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
            Span::raw("  "),
            Span::styled(
                " F3 QSO List ",
                Style::default().fg(Color::Black).bg(Color::DarkGray),
            ),
        ])),
        area,
    );
}

/// Create three spans for a label with one underlined mnemonic character.
///
/// Renders as `before` + underlined `mnemonic` + `after`, all in Cyan.
fn label_m(before: &'static str, mnemonic: char, after: &'static str) -> [Span<'static>; 3] {
    let label_style = Style::default().fg(Color::Cyan);
    [
        Span::styled(before, label_style),
        Span::styled(
            mnemonic.to_string(),
            label_style.add_modifier(Modifier::UNDERLINED),
        ),
        Span::styled(after, label_style),
    ]
}

/// Format a text field value with a fixed display width.
///
/// When `selected`, shows text padded to width without a cursor.
/// When focused (not selected), appends `|` cursor and scrolls right when long.
fn field_value(text: &str, focused: bool, selected: bool, width: usize) -> String {
    if selected {
        let len = text.chars().count();
        if len >= width {
            text.chars().take(width).collect()
        } else {
            format!("{text:<width$}")
        }
    } else {
        let mut s = text.to_string();
        if focused {
            s.push('|');
        }
        let len = s.chars().count();
        if len > width {
            s.chars().skip(len - width).collect()
        } else {
            format!("{s:<width$}")
        }
    }
}

/// Format a cycle selector value with arrow hints.
fn cycle_value(text: &str, focused: bool) -> String {
    if focused {
        format!("< {text} >")
    } else {
        format!("  {text}  ")
    }
}

/// Styled span for a text-input field value.
pub(super) fn styled_field(text: String, focused: bool, selected: bool) -> Span<'static> {
    if selected {
        Span::styled(
            text,
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
    } else if focused {
        Span::styled(
            text,
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text, Style::default().fg(Color::Gray))
    }
}

/// Styled span for a cycle selector value.
fn styled_cycle(text: String, focused: bool) -> Span<'static> {
    if focused {
        Span::styled(
            text,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(text, Style::default().fg(Color::Gray))
    }
}
