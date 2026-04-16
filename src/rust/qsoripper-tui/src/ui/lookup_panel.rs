//! Callsign lookup result panel.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::App;

/// Render the lookup result panel into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let title = if let Some(info) = &app.lookup_result {
        format!(" Lookup: {} ", info.callsign)
    } else {
        " Lookup ".to_string()
    };

    let block = Block::bordered()
        .title(title)
        .border_style(Style::default().fg(Color::Cyan));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(info) = &app.lookup_result else {
        if app.form.callsign.len() >= 3 {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "Looking up\u{2026}",
                    Style::default().fg(Color::DarkGray),
                ))),
                inner,
            );
        }
        return;
    };

    let halves =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);

    let left_area = halves.first().copied().unwrap_or(inner);
    let right_area = halves.get(1).copied().unwrap_or(inner);

    // Left column: name, QTH
    let name_line = Line::from(vec![
        Span::styled("Name  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            info.name.as_deref().unwrap_or("—"),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    let qth_line = Line::from(vec![
        Span::styled("QTH   ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            info.qth.as_deref().unwrap_or("—"),
            Style::default().fg(Color::Gray),
        ),
    ]);
    frame.render_widget(Paragraph::new(vec![name_line, qth_line]), left_area);

    // Right column: grid, CQ zone, country
    let grid_line = Line::from(vec![
        Span::styled("Grid    ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            info.grid.as_deref().unwrap_or("—"),
            Style::default().fg(Color::Cyan),
        ),
    ]);
    let zone_line = Line::from(vec![
        Span::styled("CQ Zone ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            info.cq_zone
                .map_or_else(|| "—".to_string(), |z| z.to_string()),
            Style::default().fg(Color::Gray),
        ),
        Span::raw("  "),
        Span::styled("Country ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            info.country.as_deref().unwrap_or("—"),
            Style::default().fg(Color::Gray),
        ),
    ]);
    frame.render_widget(Paragraph::new(vec![grid_line, zone_line]), right_area);
}
