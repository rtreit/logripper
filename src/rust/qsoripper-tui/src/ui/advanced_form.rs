//! Advanced QSO field entry form rendering.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;
use crate::form::Field;

/// Fixed display width for short advanced fields (power, submode, serials).
const ADV_SHORT_WIDTH: usize = 20;

/// Render the advanced field entry form into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .title(" Advanced Fields  (F2 or Esc to return) ")
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 4 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // TX power / submode
        Constraint::Length(1), // contest ID
        Constraint::Length(1), // serial sent / serial rcvd
        Constraint::Length(1), // exchange sent
        Constraint::Length(1), // exchange received
        Constraint::Fill(1),   // padding
    ])
    .split(inner);

    let form = &app.form;
    let wide = (inner.width as usize).saturating_sub(11).max(10);

    if let Some(row) = layout.first().copied() {
        render_power_submode_row(frame, row, form);
    }
    if let Some(row) = layout.get(1).copied() {
        render_contest_id_row(frame, row, form, wide);
    }
    if let Some(row) = layout.get(2).copied() {
        render_serial_row(frame, row, form);
    }
    if let Some(row) = layout.get(3).copied() {
        render_exchange_sent_row(frame, row, form, wide);
    }
    if let Some(row) = layout.get(4).copied() {
        render_exchange_rcvd_row(frame, row, form, wide);
    }
}

fn render_power_submode_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let power_val = adv_field(
        &form.tx_power,
        form.focused == Field::TxPower,
        ADV_SHORT_WIDTH,
    );
    let sub_val = adv_field(
        &form.submode_override,
        form.focused == Field::Submode,
        ADV_SHORT_WIDTH,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("TX Power "),
            styled(power_val, form.focused == Field::TxPower),
            Span::raw("   "),
            label("Submode  "),
            styled(sub_val, form.focused == Field::Submode),
        ])),
        area,
    );
}

fn render_contest_id_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm, wide: usize) {
    let cid_val = adv_field(&form.contest_id, form.focused == Field::ContestId, wide);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Contest  "),
            styled(cid_val, form.focused == Field::ContestId),
        ])),
        area,
    );
}

fn render_serial_row(frame: &mut Frame, area: Rect, form: &crate::form::LogForm) {
    let serial_sent_val = adv_field(
        &form.serial_sent,
        form.focused == Field::SerialSent,
        ADV_SHORT_WIDTH,
    );
    let serial_rcvd_val = adv_field(
        &form.serial_rcvd,
        form.focused == Field::SerialRcvd,
        ADV_SHORT_WIDTH,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Ser Sent "),
            styled(serial_sent_val, form.focused == Field::SerialSent),
            Span::raw("   "),
            label("Ser Rcvd "),
            styled(serial_rcvd_val, form.focused == Field::SerialRcvd),
        ])),
        area,
    );
}

fn render_exchange_sent_row(
    frame: &mut Frame,
    area: Rect,
    form: &crate::form::LogForm,
    wide: usize,
) {
    let es_val = adv_field(
        &form.exchange_sent,
        form.focused == Field::ExchangeSent,
        wide,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Exch Snt "),
            styled(es_val, form.focused == Field::ExchangeSent),
        ])),
        area,
    );
}

fn render_exchange_rcvd_row(
    frame: &mut Frame,
    area: Rect,
    form: &crate::form::LogForm,
    wide: usize,
) {
    let er_val = adv_field(
        &form.exchange_rcvd,
        form.focused == Field::ExchangeRcvd,
        wide,
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("Exch Rcvd"),
            styled(er_val, form.focused == Field::ExchangeRcvd),
        ])),
        area,
    );
}

/// Format an advanced field value with a fixed display width and optional cursor.
fn adv_field(text: &str, focused: bool, width: usize) -> String {
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

fn label(text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::default().fg(Color::DarkGray))
}

fn styled(text: String, focused: bool) -> Span<'static> {
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
