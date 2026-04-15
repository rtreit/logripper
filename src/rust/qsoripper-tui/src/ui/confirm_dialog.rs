//! Centered confirmation popup for destructive actions.

use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

use crate::app::App;

/// Render a "are you sure?" popup over whatever is already drawn.
pub(super) fn render(app: &App, frame: &mut Frame) {
    let subject = app
        .delete_candidate_id
        .as_deref()
        .and_then(|id| app.find_qso_by_id(id))
        .map_or_else(
            || "this QSO".to_string(),
            |q| format!("{} on {} {}", q.callsign, q.band, q.mode),
        );

    let popup_area = centered_rect(50, 8, frame.area());

    let block = Block::bordered()
        .title(" Delete QSO ")
        .title_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        .border_style(Style::default().fg(Color::Red));

    let lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::raw("  Delete "),
            Span::styled(
                subject,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("?"),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                "  [ Y / Enter: Delete ]",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::raw("   "),
            Span::styled("[ N / Esc: Cancel ]", Style::default().fg(Color::Green)),
        ]),
        Line::from(""),
    ];

    let paragraph = Paragraph::new(lines)
        .block(block)
        .alignment(Alignment::Left);

    frame.render_widget(Clear, popup_area);
    frame.render_widget(paragraph, popup_area);
}

/// Return a [`Rect`] centered within `area` with the given percentage width and fixed height.
fn centered_rect(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .flex(Flex::Center)
    .split(area);

    let mid = vertical.get(1).copied().unwrap_or(area);

    let horizontal = Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(mid);

    horizontal.get(1).copied().unwrap_or(mid)
}
