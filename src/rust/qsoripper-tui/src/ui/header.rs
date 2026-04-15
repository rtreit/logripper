//! Header bar: application title, space weather summary, and live UTC clock.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::App;

/// Render the header bar into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(32),
        Constraint::Length(36),
        Constraint::Length(28),
    ])
    .split(area);

    let title_area = cols.first().copied().unwrap_or(area);
    let rig_area = cols.get(1).copied().unwrap_or(area);
    let sw_area = cols.get(2).copied().unwrap_or(area);
    let clock_area = cols.get(3).copied().unwrap_or(area);

    // Title
    let title_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Cyan));
    let title_text = Line::from(vec![
        Span::styled(
            " QsoRipper ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            "— ham radio QSO logger",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(title_text)
            .block(title_block)
            .alignment(Alignment::Left),
        title_area,
    );

    // Rig control
    let rig_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let rig_line = rig_status_line(app);
    frame.render_widget(
        Paragraph::new(rig_line)
            .block(rig_block)
            .alignment(Alignment::Left),
        rig_area,
    );

    // Space weather
    let sw_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    let sw_line = space_weather_line(app);
    frame.render_widget(
        Paragraph::new(sw_line)
            .block(sw_block)
            .alignment(Alignment::Left),
        sw_area,
    );

    // UTC clock
    let clock_block = Block::default()
        .borders(Borders::TOP | Borders::RIGHT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Cyan));
    let utc_text = Line::from(vec![
        Span::styled("UTC ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} ", app.utc_now),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(utc_text)
            .block(clock_block)
            .alignment(Alignment::Right),
        clock_area,
    );
}

/// Build the space weather summary line for the header.
fn space_weather_line(app: &App) -> Line<'static> {
    let Some(sw) = &app.space_weather else {
        return Line::from(Span::styled(
            " Space weather unavailable",
            Style::default().fg(Color::DarkGray),
        ));
    };

    let k_str = sw
        .k_index
        .map_or_else(|| "K=?".to_string(), |k| format!("K={k:.0}"));
    let solar_str = sw
        .solar_flux
        .map_or_else(|| "SFI=?".to_string(), |sf| format!("SFI={sf:.0}"));
    let spots_str = sw
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
        Span::raw(" "),
        Span::styled(
            k_str,
            Style::default().fg(k_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(solar_str, Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled(spots_str, Style::default().fg(Color::Yellow)),
    ])
}

/// Build the rig control status line for the header.
fn rig_status_line(app: &App) -> Line<'static> {
    use crate::app::RigStatus;

    if !app.rig_control_enabled {
        return Line::from(Span::styled(
            " Rig: OFF",
            Style::default().fg(Color::DarkGray),
        ));
    }

    let Some(ref rig) = app.rig_info else {
        return Line::from(Span::styled(
            " Rig: waiting…",
            Style::default().fg(Color::DarkGray),
        ));
    };

    let (status_label, status_color) = match rig.status {
        RigStatus::Connected => ("●", Color::Green),
        RigStatus::Disconnected => ("○", Color::Yellow),
        RigStatus::Error => ("✖", Color::Red),
        RigStatus::Disabled => ("–", Color::DarkGray),
    };

    if rig.status != RigStatus::Connected {
        let label = match rig.status {
            RigStatus::Disconnected => "disconnected",
            RigStatus::Error => rig.error_message.as_deref().unwrap_or("error"),
            RigStatus::Disabled => "disabled",
            RigStatus::Connected => unreachable!(),
        };
        return Line::from(vec![
            Span::raw(" "),
            Span::styled(
                status_label.to_string(),
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(label.to_string(), Style::default().fg(status_color)),
        ]);
    }

    let freq = &rig.frequency_display;
    let mode = rig
        .mode
        .as_deref()
        .or(rig.submode.as_deref())
        .unwrap_or("?");

    Line::from(vec![
        Span::raw(" "),
        Span::styled(
            status_label.to_string(),
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            freq.clone(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(mode.to_string(), Style::default().fg(Color::Cyan)),
    ])
}
