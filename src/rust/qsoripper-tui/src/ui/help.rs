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
    let popup = centered_rect(65, 90, area);
    frame.render_widget(Clear, popup);

    let block = Block::bordered().title(" Help ").border_style(
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    );

    let key_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(Color::Gray);
    let head_style = Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line> = vec![Line::raw("")];

    let section = |title: &'static str| Line::from(vec![Span::styled(title, head_style)]);

    let binding = |key: &'static str, desc: &'static str| {
        Line::from(vec![
            Span::raw("  "),
            Span::styled(key, key_style),
            Span::raw("   "),
            Span::styled(desc, desc_style),
        ])
    };

    lines.push(section("  Navigation"));
    lines.push(binding("Tab / Shift+Tab  ", "Next / previous field"));
    lines.push(binding("Left / Right     ", "Cycle Band or Mode value"));
    lines.push(binding("Typing a char    ", "Type-select Band or Mode"));
    lines.push(binding("F1               ", "This help screen"));
    lines.push(binding(
        "F2               ",
        "Toggle advanced fields (F2 or Esc to close)",
    ));
    lines.push(binding("F10 / Alt+Enter  ", "Log the QSO"));
    lines.push(binding("F7               ", "Reset QSO start time to now"));
    lines.push(binding(
        "F8               ",
        "Toggle rig control (read freq/mode from radio)",
    ));
    lines.push(binding(
        "Esc              ",
        "Clear form (or close advanced view)",
    ));
    lines.push(binding("Ctrl+Q           ", "Quit"));
    lines.push(Line::raw(""));

    lines.push(section("  Alt+key: Jump to field"));
    let jumps: &[(&str, &str)] = &[
        ("Alt+C  ", "Callsign"),
        ("Alt+B  ", "Band"),
        ("Alt+M  ", "Mode"),
        ("Alt+S  ", "RST Sent"),
        ("Alt+R  ", "RST Rcvd"),
        ("Alt+O  ", "Comment"),
        ("Alt+N  ", "Notes"),
        ("Alt+F  ", "Frequency MHz"),
        ("Alt+D  ", "Date"),
        ("Alt+T  ", "Time"),
    ];
    for (key, desc) in jumps {
        lines.push(binding(key, desc));
    }
    lines.push(Line::raw(""));

    lines.push(section("  Recent QSOs panel (F3)"));
    lines.push(binding("F3               ", "Focus the QSO list"));
    lines.push(binding(
        "\u{2191} / \u{2193}            ",
        "Navigate QSO entries",
    ));
    lines.push(binding("Enter            ", "Load selected QSO into form"));
    lines.push(binding("Esc / F3         ", "Return focus to form"));
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
