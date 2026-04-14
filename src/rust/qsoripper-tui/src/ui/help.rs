//! Help overlay rendered as a centered popup.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Clear, Paragraph},
    Frame,
};

/// Render the help overlay centered over `area`.
pub(super) fn render(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered().title(" Help ").border_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let bindings: &[(&str, &str)] = &[
        ("Tab / Shift+Tab", "Next / previous field"),
        ("Left / Right   ", "Cycle Band or Mode"),
        ("F1             ", "This help screen"),
        ("F2             ", "Toggle advanced fields"),
        ("F10            ", "Log the QSO"),
        ("Esc            ", "Clear form (or close advanced)"),
        ("Ctrl+Q         ", "Quit"),
    ];

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);

    let mut lines: Vec<Line> = vec![Line::raw("")];
    for (key, desc) in bindings {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(*key, key_style),
            Span::raw("   "),
            Span::styled(*desc, desc_style),
        ]));
    }
    lines.push(Line::raw(""));
    lines.push(Line::from(Span::styled(
        "       Press any key to close",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::ITALIC),
    )));

    frame.render_widget(Paragraph::new(lines).block(block), popup);
}

/// Compute a centered [`Rect`] that is `percent_x`% wide and `percent_y`% tall within `r`.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let margin_v = (100 - percent_y) / 2;
    let margin_h = (100 - percent_x) / 2;

    let vertical = Layout::vertical([
        Constraint::Percentage(margin_v),
        Constraint::Percentage(percent_y),
        Constraint::Percentage(margin_v),
    ])
    .split(r);

    let middle = vertical.get(1).copied().unwrap_or(r);

    let horizontal = Layout::horizontal([
        Constraint::Percentage(margin_h),
        Constraint::Percentage(percent_x),
        Constraint::Percentage(margin_h),
    ])
    .split(middle);

    horizontal.get(1).copied().unwrap_or(middle)
}
