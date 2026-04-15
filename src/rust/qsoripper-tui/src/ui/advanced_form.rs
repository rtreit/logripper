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
    reason = "renders six content rows plus a hint line for the main tab"
)]
fn render_main_tab(frame: &mut Frame, area: Rect, form: &crate::form::LogForm, wide: usize) {
    if area.height == 0 {
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(1), // callsign / band / mode
        Constraint::Length(1), // freq / date / time / time-off
        Constraint::Length(1), // qth / name
        Constraint::Length(1), // rst sent / rst rcvd
        Constraint::Length(1), // comment
        Constraint::Length(1), // notes
        Constraint::Fill(1),   // hint
    ])
    .split(area);

    // Row 0: [C]allsign  [B]and  [M]ode
    if let Some(row) = rows.first().copied() {
        let cs_focused = form.focused == Field::Callsign;
        let cs_selected = cs_focused && form.field_selected;
        let band_focused = form.focused == Field::Band;
        let mode_focused = form.focused == Field::Mode;
        let cs_val = adv_field(&form.callsign, cs_focused, cs_selected, 12);
        let band_val = cycle_adv(form.band_str(), band_focused);
        let mode_val = cycle_adv(form.mode_str(), mode_focused);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(kl("", 'c', "allsign "));
        spans.push(styled_field(cs_val, cs_focused, cs_selected));
        spans.push(Span::raw("  "));
        spans.extend(kl("", 'b', "and "));
        spans.push(Span::styled(
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
        ));
        spans.push(Span::raw("  "));
        spans.extend(kl("", 'm', "ode "));
        spans.push(Span::styled(
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
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Row 1: [F]req  [D]ate  [T]ime  T[e]nd (time off)
    if let Some(row) = rows.get(1).copied() {
        let ff = form.focused == Field::FrequencyMhz;
        let fs = ff && form.field_selected;
        let df = form.focused == Field::Date;
        let ds = df && form.field_selected;
        let tf = form.focused == Field::Time;
        let ts = tf && form.field_selected;
        let ef = form.focused == Field::TimeOff;
        let es = ef && form.field_selected;
        let fv = adv_field(&form.frequency_mhz, ff, fs, 9);
        let dv = adv_field(&form.date, df, ds, 10);
        let tv = adv_field(&form.time, tf, ts, 8);
        let ev = adv_field(&form.time_off, ef, es, 8);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(kl("", 'f', "req "));
        spans.push(styled_field(fv, ff, fs));
        spans.push(Span::raw("  "));
        spans.extend(kl("", 'd', "ate "));
        spans.push(styled_field(dv, df, ds));
        spans.push(Span::raw("  "));
        spans.extend(kl("", 't', "ime "));
        spans.push(styled_field(tv, tf, ts));
        spans.push(Span::raw("  "));
        spans.extend(kl("T", 'e', "nd  "));
        spans.push(styled_field(ev, ef, es));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Row 2: [Q]th  N[a]me
    if let Some(row) = rows.get(2).copied() {
        let qf = form.focused == Field::Qth;
        let qs = qf && form.field_selected;
        let nf = form.focused == Field::WorkedName;
        let ns = nf && form.field_selected;
        let half = wide.saturating_sub(8) / 2;
        let qv = adv_field(&form.qth, qf, qs, half.max(8));
        let nv = adv_field(&form.worked_name, nf, ns, half.max(8));
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(kl("", 'q', "th      "));
        spans.push(styled_field(qv, qf, qs));
        spans.push(Span::raw("  "));
        spans.extend(kl("N", 'a', "me     "));
        spans.push(styled_field(nv, nf, ns));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Row 3: RST [S]nt  RST [R]cv
    if let Some(row) = rows.get(3).copied() {
        let sf = form.focused == Field::RstSent;
        let ss = sf && form.field_selected;
        let rf = form.focused == Field::RstRcvd;
        let rs = rf && form.field_selected;
        let sv = adv_field(&form.rst_sent, sf, ss, 5);
        let rv = adv_field(&form.rst_rcvd, rf, rs, 5);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(kl("RST ", 's', "nt  "));
        spans.push(styled_field(sv, sf, ss));
        spans.push(Span::raw("    "));
        spans.extend(kl("RST ", 'r', "cv  "));
        spans.push(styled_field(rv, rf, rs));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Row 4: C[o]mment
    if let Some(row) = rows.get(4).copied() {
        let focused = form.focused == Field::Comment;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.comment, focused, selected, wide);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(kl("C", 'o', "mment  "));
        spans.push(styled_field(val, focused, selected));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Row 5: [N]otes
    if let Some(row) = rows.get(5).copied() {
        let focused = form.focused == Field::Notes;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.notes, focused, selected, wide);
        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend(kl("", 'n', "otes   "));
        spans.push(styled_field(val, focused, selected));
        frame.render_widget(Paragraph::new(Line::from(spans)), row);
    }

    // Hint row
    if let Some(row) = rows.get(6).copied() {
        frame.render_widget(
            Paragraph::new(Span::styled(
                "Alt+key to jump to field  |  Tab/ShiftTab to navigate  |  F5/F6 to switch tabs",
                Style::default().fg(Color::DarkGray),
            )),
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
    if let Some(row) = rows.get(3).copied() {
        let focused = form.focused == Field::Skcc;
        let selected = focused && form.field_selected;
        let val = adv_field(&form.skcc, focused, selected, short);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                label("SKCC     "),
                styled_field(val, focused, selected),
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

/// Build a label with one underlined shortcut key character.
///
/// `prefix` appears before the key, `suffix` after it. The key renders in yellow+underlined;
/// prefix and suffix render in cyan. All three together form the complete label.
fn kl(prefix: &'static str, key: char, suffix: &'static str) -> [Span<'static>; 3] {
    let label_style = Style::default().fg(Color::Cyan);
    [
        Span::styled(prefix, label_style),
        Span::styled(
            key.to_ascii_uppercase().to_string(),
            label_style.add_modifier(Modifier::UNDERLINED),
        ),
        Span::styled(suffix, label_style),
    ]
}

fn cycle_adv(text: &str, focused: bool) -> String {
    if focused {
        format!("< {text} >")
    } else {
        format!("  {text}  ")
    }
}
