//! Footer bar showing key binding hints.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

/// Render the key-binding hint footer into `area`.
pub(super) fn render(frame: &mut Frame, area: Rect) {
    let hints: &[(&str, &str)] = &[
        ("Tab", "Next field"),
        ("\u{2190}/\u{2192}", "Cycle Band/Mode"),
        ("F1", "Help"),
        ("F2", "Advanced"),
        ("F3", "QSO List"),
        ("F4", "Search"),
        ("F5/F6", "Adv tabs"),
        ("F8", "Rig ctrl"),
        ("F10", "Log QSO (Alt+Enter)"),
        ("Esc", "Clear"),
        ("Ctrl+Q", "Quit"),
    ];

    let mut spans: Vec<Span> = Vec::new();
    for (i, (key, desc)) in hints.iter().enumerate() {
        if i > 0 {
            spans.push(Span::raw("  "));
        }
        spans.push(Span::styled(
            *key,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(*desc, Style::default().fg(Color::DarkGray)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}
