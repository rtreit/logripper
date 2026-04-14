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
const TIME_WIDTH: usize = 5;

/// Render the QSO log entry form into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .title(" New QSO ")
        .border_style(Style::default().fg(Color::Yellow));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 7 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // callsign / band / mode
        Constraint::Length(1), // rst sent / rst rcvd
        Constraint::Length(1), // comment
        Constraint::Length(1), // notes
        Constraint::Length(1), // freq / date / time
        Constraint::Fill(1),   // padding
        Constraint::Length(1), // hints
    ])
    .split(inner);

    let cs_area = layout.first().copied().unwrap_or(inner);
    let rst_area = layout.get(1).copied().unwrap_or(inner);
    let comment_area = layout.get(2).copied().unwrap_or(inner);
    let notes_area = layout.get(3).copied().unwrap_or(inner);
    let freq_area = layout.get(4).copied().unwrap_or(inner);
    let hints_area = layout.get(6).copied().unwrap_or(inner);

    let form = &app.form;

    render_callsign_row(frame, cs_area, form);
    render_rst_row(frame, rst_area, form);
    render_comment_row(frame, comment_area, form);
    render_notes_row(frame, notes_area, form);
    render_freq_row(frame, freq_area, form);
    render_hints_row(frame, hints_area);
}

/// Render the callsign / band / mode row.
fn render_callsign_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let cs_val = field_value(
        &form.callsign,
        form.focused == Field::Callsign,
        CALLSIGN_WIDTH,
    );
    let band_val = cycle_value(form.band_str(), form.focused == Field::Band);
    let mode_val = cycle_value(form.mode_str(), form.focused == Field::Mode);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Callsign "),
            styled_field(cs_val, form.focused == Field::Callsign),
            Span::raw("  "),
            label("Band "),
            styled_cycle(band_val, form.focused == Field::Band),
            Span::raw("  "),
            label("Mode "),
            styled_cycle(mode_val, form.focused == Field::Mode),
        ])),
        area,
    );
}

/// Render the RST sent / RST received row.
fn render_rst_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let sent_val = field_value(&form.rst_sent, form.focused == Field::RstSent, RST_WIDTH);
    let rcvd_val = field_value(&form.rst_rcvd, form.focused == Field::RstRcvd, RST_WIDTH);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("RST Snt  "),
            styled_field(sent_val, form.focused == Field::RstSent),
            Span::raw("   "),
            label("RST Rcvd "),
            styled_field(rcvd_val, form.focused == Field::RstRcvd),
        ])),
        area,
    );
}

/// Render the comment row.
fn render_comment_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let label_len: usize = 9;
    let width = (area.width as usize).saturating_sub(label_len + 2).max(10);
    let val = field_value(&form.comment, form.focused == Field::Comment, width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Comment  "),
            styled_field(val, form.focused == Field::Comment),
        ])),
        area,
    );
}

/// Render the notes row.
fn render_notes_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let label_len: usize = 9;
    let width = (area.width as usize).saturating_sub(label_len + 2).max(10);
    let val = field_value(&form.notes, form.focused == Field::Notes, width);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Notes    "),
            styled_field(val, form.focused == Field::Notes),
        ])),
        area,
    );
}

/// Render the frequency / date / time row.
fn render_freq_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let freq_val = field_value(
        &form.frequency_mhz,
        form.focused == Field::FrequencyMhz,
        FREQ_WIDTH,
    );
    let date_val = field_value(&form.date, form.focused == Field::Date, DATE_WIDTH);
    let time_val = field_value(&form.time, form.focused == Field::Time, TIME_WIDTH);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Freq MHz "),
            styled_field(freq_val, form.focused == Field::FrequencyMhz),
            Span::raw("  "),
            label("Date "),
            styled_field(date_val, form.focused == Field::Date),
            Span::raw("  "),
            label("Time "),
            styled_field(time_val, form.focused == Field::Time),
        ])),
        area,
    );
}

/// Render the keyboard-hints row at the bottom of the form.
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
        ])),
        area,
    );
}

/// Format a text field value with a fixed display width.
///
/// Shows the rightmost `width` characters when the content (plus cursor) exceeds `width`,
/// giving a natural scroll-to-end effect during typing. Pads with spaces when shorter.
fn field_value(text: &str, focused: bool, width: usize) -> String {
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

/// Format a cycle selector value with arrow hints.
fn cycle_value(text: &str, focused: bool) -> String {
    if focused {
        format!("< {text} >")
    } else {
        format!("  {text}  ")
    }
}

/// Styled span for a label.
fn label(text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::default().fg(Color::DarkGray))
}

/// Styled span for a text-input field value.
fn styled_field(text: String, focused: bool) -> Span<'static> {
    if focused {
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
