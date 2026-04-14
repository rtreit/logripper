//! Advanced QSO field entry form rendering — tabbed layout.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;
use crate::form::{AdvancedTab, Field};
use crate::ui::log_form::styled_field;

/// Render the advanced field entry form into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::bordered()
        .title(" Advanced Fields  (F2/Esc to return | F5/F6 to switch tabs) ")
        .border_style(Style::default().fg(Color::Magenta));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.height < 3 {
        return;
    }

    let layout = Layout::vertical([
        Constraint::Length(1), // tab bar
        Constraint::Length(1), // separator line
        Constraint::Fill(1),   // field rows
    ])
    .split(inner);

    let tab_area = layout.first().copied().unwrap_or(inner);
    let fields_area = layout.get(2).copied().unwrap_or(inner);

    render_tab_bar(frame, tab_area, app.form.advanced_tab);

    let form = &app.form;
    let wide = (inner.width as usize).saturating_sub(13).max(10);

    match form.advanced_tab {
        AdvancedTab::Main => render_main_tab(frame, fields_area, form, wide),
        AdvancedTab::Contest => render_contest_tab(frame, fields_area, form, wide),
        AdvancedTab::Technical => render_technical_tab(frame, fields_area, form, wide),
        AdvancedTab::Awards => render_awards_tab(frame, fields_area, form, wide),
    }
}

fn render_tab_bar(frame: &mut Frame, area: Rect, active: AdvancedTab) {
    let mut spans: Vec<Span<'static>> = Vec::new();
    for &tab in AdvancedTab::ALL {
        let label = format!(" {} ", tab.label());
        if tab == active {
            spans.push(Span::styled(
                label,
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Magenta)
                    .add_modifier(Modifier::BOLD),
            ));
        } else {
            spans.push(Span::styled(
                label,
                Style::default().fg(Color::Magenta).bg(Color::Reset),
            ));
        }
        spans.push(Span::raw(" "));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[expect(
    clippy::too_many_lines,
    reason = "renders six field rows for the main tab"
)]
fn render_main_tab(frame: &mut Frame, area: Rect, form: &crate::form::LogForm, wide: usize) {
    if area.height == 0 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1), // callsign / band / mode
        Constraint::Length(1), // freq / date
        Constraint::Length(1), // time / time-off / qth
        Constraint::Length(1), // rst sent / rst rcvd
        Constraint::Length(1), // comment
        Constraint::Length(1), // notes
        Constraint::Fill(1),
    ])
    .split(area);

    if let Some(row) = rows.first().copied() {
        let cs_focused = form.focused == Field::Callsign;
        let cs_selected = cs_focused && form.field_selected;
        let band_focused = form.focused == Field::Band;
        let mode_focused = form.focused == Field::Mode;
        let cs_val = adv_field(&form.callsign, cs_focused, cs_selected, 12);
        let band_val = cycle_adv(form.band_str(), band_focused);
        let mode_val = cycle_adv(form.mode_str(), mode_focused);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Callsign "),
                styled_field(cs_val, cs_focused, cs_selected),
                Span::raw("  "),
                label("Band "),
                Span::styled(
                    band_val,
                    Style::default()
                        .fg(if band_focused {
                            Color::Yellow
                        } else {
                            Color::Gray
                        })
                        .add_modifier(if band_focused {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::raw("  "),
                label("Mode "),
                Span::styled(
                    mode_val,
                    Style::default()
                        .fg(if mode_focused {
                            Color::Yellow
                        } else {
                            Color::Gray
                        })
                        .add_modifier(if mode_focused {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(1).copied() {
        let freq_focused = form.focused == Field::FrequencyMhz;
        let freq_selected = freq_focused && form.field_selected;
        let date_focused = form.focused == Field::Date;
        let date_selected = date_focused && form.field_selected;
        let freq_val = adv_field(&form.frequency_mhz, freq_focused, freq_selected, 10);
        let date_val = adv_field(&form.date, date_focused, date_selected, 10);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Freq MHz "),
                styled_field(freq_val, freq_focused, freq_selected),
                Span::raw("  "),
                label("Date     "),
                styled_field(date_val, date_focused, date_selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(2).copied() {
        let time_focused = form.focused == Field::Time;
        let time_selected = time_focused && form.field_selected;
        let toff_focused = form.focused == Field::TimeOff;
        let toff_selected = toff_focused && form.field_selected;
        let qth_focused = form.focused == Field::Qth;
        let qth_selected = qth_focused && form.field_selected;
        let time_val = adv_field(&form.time, time_focused, time_selected, 8);
        let toff_val = adv_field(&form.time_off, toff_focused, toff_selected, 8);
        let qth_val = adv_field(&form.qth, qth_focused, qth_selected, 15);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Time On  "),
                styled_field(time_val, time_focused, time_selected),
                Span::raw("  "),
                label("Time Off "),
                styled_field(toff_val, toff_focused, toff_selected),
                Span::raw("  "),
                label("QTH      "),
                styled_field(qth_val, qth_focused, qth_selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(3).copied() {
        let sent_focused = form.focused == Field::RstSent;
        let sent_selected = sent_focused && form.field_selected;
        let rcvd_focused = form.focused == Field::RstRcvd;
        let rcvd_selected = rcvd_focused && form.field_selected;
        let sent_val = adv_field(&form.rst_sent, sent_focused, sent_selected, 5);
        let rcvd_val = adv_field(&form.rst_rcvd, rcvd_focused, rcvd_selected, 5);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("RST Sent "),
                styled_field(sent_val, sent_focused, sent_selected),
                Span::raw("   "),
                label("RST Rcvd "),
                styled_field(rcvd_val, rcvd_focused, rcvd_selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(4).copied() {
        let focused = form.focused == Field::Comment;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.comment, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Comment  "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(5).copied() {
        let focused = form.focused == Field::Notes;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.notes, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Notes    "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
}

fn render_contest_tab(frame: &mut Frame, area: Rect, form: &crate::form::LogForm, wide: usize) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    let short = 20_usize;

    if let Some(row) = rows.first().copied() {
        let pf = form.focused == Field::TxPower;
        let ps = pf && form.field_selected;
        let sf = form.focused == Field::Submode;
        let ss = sf && form.field_selected;
        let pv = adv_field(&form.tx_power, pf, ps, short);
        let sv = adv_field(&form.submode_override, sf, ss, short);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("TX Power "),
                styled_field(pv, pf, ps),
                Span::raw("   "),
                label("Submode  "),
                styled_field(sv, sf, ss),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(1).copied() {
        let focused = form.focused == Field::ContestId;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.contest_id, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Contest  "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(2).copied() {
        let sf = form.focused == Field::SerialSent;
        let ss = sf && form.field_selected;
        let rf = form.focused == Field::SerialRcvd;
        let rs = rf && form.field_selected;
        let sv = adv_field(&form.serial_sent, sf, ss, short);
        let rv = adv_field(&form.serial_rcvd, rf, rs, short);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Ser Sent "),
                styled_field(sv, sf, ss),
                Span::raw("   "),
                label("Ser Rcvd "),
                styled_field(rv, rf, rs),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(3).copied() {
        let focused = form.focused == Field::ExchangeSent;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.exchange_sent, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Exch Snt "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(4).copied() {
        let focused = form.focused == Field::ExchangeRcvd;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.exchange_rcvd, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Exch Rcvd"),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
}

fn render_technical_tab(frame: &mut Frame, area: Rect, form: &crate::form::LogForm, wide: usize) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    if let Some(row) = rows.first().copied() {
        let focused = form.focused == Field::PropMode;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.prop_mode, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Prop Mode"),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(1).copied() {
        let focused = form.focused == Field::SatName;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.sat_name, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Sat Name "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(2).copied() {
        let focused = form.focused == Field::SatMode;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.sat_mode, focused, selected, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("Sat Mode "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
}

fn render_awards_tab(frame: &mut Frame, area: Rect, form: &crate::form::LogForm, wide: usize) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Fill(1),
    ])
    .split(area);

    let short = 20_usize;

    if let Some(row) = rows.first().copied() {
        let focused = form.focused == Field::Iota;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.iota, focused, selected, short);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("IOTA     "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(1).copied() {
        let focused = form.focused == Field::ArrlSection;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.arrl_section, focused, selected, short);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("ARRL Sec "),
                styled_field(val, focused, selected),
            ])),
            row,
        );
    }
    if let Some(row) = rows.get(2).copied() {
        let wf = form.focused == Field::WorkedState;
        let ws = wf && form.field_selected;
        let cf = form.focused == Field::WorkedCounty;
        let cs = cf && form.field_selected;
        let wv = adv_field(&form.worked_state, wf, ws, short);
        let cv = adv_field(&form.worked_county, cf, cs, wide);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("State    "),
                styled_field(wv, wf, ws),
                Span::raw("   "),
                label("County   "),
                styled_field(cv, cf, cs),
            ])),
            row,
        );
    }
}

/// Format an advanced field value with a fixed display width and optional cursor.
fn adv_field(text: &str, focused: bool, selected: bool, width: usize) -> String {
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

fn label(text: &str) -> Span<'static> {
    Span::styled(text.to_string(), Style::default().fg(Color::Cyan))
}

fn cycle_adv(text: &str, focused: bool) -> String {
    if focused {
        format!("< {text} >")
    } else {
        format!("  {text}  ")
    }
}
