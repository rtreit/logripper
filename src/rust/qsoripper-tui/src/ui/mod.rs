//! UI rendering — header, form, lookup panel, recent QSOs, footer, and help overlay.

mod advanced_form;
mod confirm_dialog;
mod footer;
mod header;
mod help;
mod log_form;
mod lookup_panel;
mod recent_qsos;

use ratatui::{
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Paragraph},
    Frame,
};

use crate::app::{App, View};

/// Render the full TUI layout into the given frame.
pub(crate) fn render_ui(app: &App, frame: &mut Frame) {
    let area = frame.area();

    let chunks = Layout::vertical([
        Constraint::Length(3),  // header (title + space weather + clock)
        Constraint::Length(3),  // status bar
        Constraint::Length(12), // log form
        Constraint::Length(5),  // lookup panel
        Constraint::Fill(1),    // recent QSOs
        Constraint::Length(1),  // footer
    ])
    .split(area);

    let header_area = chunks.first().copied().unwrap_or(area);
    let status_area = chunks.get(1).copied().unwrap_or(area);
    let form_area = chunks.get(2).copied().unwrap_or(area);
    let lookup_area = chunks.get(3).copied().unwrap_or(area);
    let recent_area = chunks.get(4).copied().unwrap_or(area);
    let footer_area = chunks.get(5).copied().unwrap_or(area);

    header::render(app, frame, header_area);
    render_status_bar(app, frame, status_area);
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

    if matches!(app.view, View::ConfirmDeleteQso) {
        confirm_dialog::render(app, frame);
    }
}

/// Render the status bar (QSO log / error feedback).
fn render_status_bar(app: &App, frame: &mut Frame, area: ratatui::layout::Rect) {
    let block = Block::bordered()
        .title(" Status ")
        .border_style(Style::default().fg(Color::Cyan));

    let text = if let Some(msg) = &app.status_message {
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

    frame.render_widget(Paragraph::new(text).block(block), area);
}
