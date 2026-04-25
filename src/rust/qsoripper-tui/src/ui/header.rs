//! Header bar: application title, space weather summary, and live UTC clock.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, EngineStatus};

/// Render the header bar into `area`.
pub(super) fn render(app: &App, frame: &mut Frame, area: Rect) {
    let cols = Layout::horizontal([
        Constraint::Length(13),
        Constraint::Fill(1),
        Constraint::Length(28),
        Constraint::Length(30),
        Constraint::Length(26),
    ])
    .split(area);

    let title_area = cols.first().copied().unwrap_or(area);
    let engine_area = cols.get(1).copied().unwrap_or(area);
    let rig_area = cols.get(2).copied().unwrap_or(area);
    let sw_area = cols.get(3).copied().unwrap_or(area);
    let clock_area = cols.get(4).copied().unwrap_or(area);

    // Title.
    let title_block = Block::default()
        .borders(Borders::TOP | Borders::LEFT | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Cyan));
    let title_text = Line::from(vec![Span::styled(
        " QsoRipper ",
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )]);
    frame.render_widget(
        Paragraph::new(title_text)
            .block(title_block)
            .alignment(Alignment::Left),
        title_area,
    );

    // Engine reachability — sits next to the rig status so a downed server is
    // impossible to miss.
    let engine_block = Block::default()
        .borders(Borders::TOP | Borders::BOTTOM)
        .border_style(Style::default().fg(Color::Cyan));
    let engine_line = Line::from(engine_status_spans(app));
    frame.render_widget(
        Paragraph::new(engine_line)
            .block(engine_block)
            .alignment(Alignment::Left),
        engine_area,
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

/// Build the engine reachability spans rendered next to the title.
///
/// Returns spans (rather than a full `Line`) so the caller can append them to
/// the existing title content without owning a separate header column.
fn engine_status_spans(app: &App) -> Vec<Span<'static>> {
    match &app.engine_status {
        EngineStatus::Connected => vec![
            Span::styled(
                "●",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled("engine", Style::default().fg(Color::Green)),
        ],
        EngineStatus::Unreachable { .. } => vec![
            Span::styled(
                "✖",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!("engine unreachable @ {}", app.endpoint),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ],
        EngineStatus::Unknown => vec![
            Span::styled("○", Style::default().fg(Color::DarkGray)),
            Span::raw(" "),
            Span::styled("engine ...", Style::default().fg(Color::DarkGray)),
        ],
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use ratatui::{backend::TestBackend, layout::Rect, style::Color, Terminal};

    use super::{engine_status_spans, render};
    use crate::app::{App, EngineStatus};

    fn make_app() -> App {
        App::new("http://127.0.0.1:50051".to_string())
    }

    fn header_text(app: &App) -> String {
        let backend = TestBackend::new(160, 3);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| render(app, f, Rect::new(0, 0, 160, 3)))
            .unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                out.push_str(buffer[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    #[test]
    fn engine_unreachable_renders_red_message_with_endpoint() {
        let mut app = make_app();
        app.engine_status = EngineStatus::Unreachable {
            message: "transport error".to_string(),
        };
        let text = header_text(&app);
        assert!(
            text.contains("engine unreachable @ http://127.0.0.1:50051"),
            "expected unreachable banner with endpoint, got:\n{text}"
        );
        assert!(text.contains('✖'), "expected red ✖ glyph, got:\n{text}");

        let spans = engine_status_spans(&app);
        assert!(spans.iter().any(|s| s.style.fg == Some(Color::Red)));
    }

    #[test]
    fn engine_connected_renders_green_indicator() {
        let mut app = make_app();
        app.engine_status = EngineStatus::Connected;
        let text = header_text(&app);
        assert!(text.contains('●'), "expected green ● glyph, got:\n{text}");
        assert!(
            text.contains("engine"),
            "expected 'engine' label, got:\n{text}"
        );

        let spans = engine_status_spans(&app);
        assert!(spans.iter().any(|s| s.style.fg == Some(Color::Green)));
    }

    #[test]
    fn engine_unknown_renders_dim_indicator() {
        let app = make_app();
        let text = header_text(&app);
        assert!(text.contains('○'), "expected dim ○ glyph, got:\n{text}");
        assert!(
            text.contains("engine ..."),
            "expected 'engine ...' placeholder, got:\n{text}"
        );

        let spans = engine_status_spans(&app);
        assert!(spans.iter().any(|s| s.style.fg == Some(Color::DarkGray)));
    }
}
