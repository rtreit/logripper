//! UI rendering — header, form, lookup panel, recent QSOs, footer, and help overlay.

mod advanced_form;
mod footer;
mod header;
mod help;
mod log_form;
mod lookup_panel;
mod recent_qsos;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    Frame,
};

use crate::app::{App, View};

/// Render the full TUI layout into the given frame.
pub(crate) fn render_ui(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::vertical([
        Constraint::Length(3),  // header
        Constraint::Length(3),  // space weather / status row
        Constraint::Length(12), // log form
        Constraint::Length(5),  // lookup panel
        Constraint::Fill(1),    // recent QSOs
        Constraint::Length(1),  // footer
    ])
    .split(area);

    let header_area = chunks.first().copied().unwrap_or(area);
    let info_area = chunks.get(1).copied().unwrap_or(area);
    let form_area = chunks.get(2).copied().unwrap_or(area);
    let lookup_area = chunks.get(3).copied().unwrap_or(area);
    let recent_area = chunks.get(4).copied().unwrap_or(area);
    let footer_area = chunks.get(5).copied().unwrap_or(area);

    header::render(app, frame, header_area);
    render_info_row(app, frame, info_area);
    if matches!(app.view, View::Advanced) {
        advanced_form::render(app, frame, form_area);
    } else {
        log_form::render(app, frame, form_area);
    }
    lookup_panel::render(app, frame, lookup_area);
    recent_qsos::render(app, frame, recent_area);
    footer::render(frame, footer_area);

    if matches!(app.view, View::Help) {
        help::render(frame, area);
    }
}

/// Render the info row containing space weather and a status message tile.
fn render_info_row(app: &App, frame: &mut Frame, area: Rect) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Paragraph},
    };

    let halves =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(area);

    let sw_area = halves.first().copied().unwrap_or(area);
    let status_area = halves.get(1).copied().unwrap_or(area);

    // Space weather tile
    let sw_block = Block::bordered()
        .title(" Space Weather ")
        .border_style(Style::default().fg(Color::Cyan));
    let sw_text = if let Some(sw) = &app.space_weather {
        let k_str = sw
            .k_index
            .map_or_else(|| "K=?".to_string(), |k| format!("K={k:.0}"));
        let sf_str = sw
            .solar_flux
            .map_or_else(|| "SFI=?".to_string(), |sf| format!("SFI={sf:.0}"));
        let sun_str = sw
            .sunspot_number
            .map_or_else(|| "SN=?".to_string(), |sn| format!("SN={sn}"));

        let k_color = sw.k_index.map_or(Color::DarkGray, |k| {
            if k <= 3.0 {
                Color::Green
            } else if k <= 5.0 {
                Color::Yellow
            } else {
                Color::Red
            }
        });

        Line::from(vec![
            Span::styled(
                k_str,
                Style::default().fg(k_color).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled(sf_str, Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled(sun_str, Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled(&sw.status, Style::default().fg(Color::DarkGray)),
        ])
    } else {
        Line::from(Span::styled(
            "Not available",
            Style::default().fg(Color::DarkGray),
        ))
    };
    frame.render_widget(Paragraph::new(sw_text).block(sw_block), sw_area);

    // Status / station tile
    let status_block = Block::bordered()
        .title(" Status ")
        .border_style(Style::default().fg(Color::Cyan));
    let status_text = if let Some(msg) = &app.status_message {
        let color = if msg.is_error {
            Color::Red
        } else {
            Color::Green
        };
        Line::from(Span::styled(
            &msg.text,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ))
    } else {
        Line::from(Span::styled("Ready", Style::default().fg(Color::DarkGray)))
    };
    frame.render_widget(Paragraph::new(status_text).block(status_block), status_area);
}
