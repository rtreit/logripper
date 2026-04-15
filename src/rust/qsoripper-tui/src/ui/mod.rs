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
        Constraint::Fill(1),    // recent QSOs (with embedded search)
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use ratatui::{backend::TestBackend, Terminal};

    use crate::app::{App, CallsignInfo, RecentQso, SpaceWeatherInfo, View};
    use crate::form::AdvancedTab;

    fn make_terminal() -> Terminal<TestBackend> {
        let backend = TestBackend::new(120, 40);
        Terminal::new(backend).unwrap()
    }

    fn make_app() -> App {
        App::new("http://localhost:50051".to_string())
    }

    fn make_qso(id: &str, callsign: &str) -> RecentQso {
        RecentQso {
            local_id: id.to_string(),
            utc: "14:32".to_string(),
            callsign: callsign.to_string(),
            band: "20M".to_string(),
            mode: "SSB".to_string(),
            rst_sent: "59".to_string(),
            rst_rcvd: "59".to_string(),
            country: Some("United States".to_string()),
            grid: Some("CN87".to_string()),
            name: Some("John Smith".to_string()),
        }
    }

    #[test]
    fn render_log_entry_view() {
        let mut terminal = make_terminal();
        let app = make_app();
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_log_entry_editing_mode() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.editing_local_id = Some("qso-id-123".to_string());
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_advanced_main_tab() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::Advanced;
        app.form.advanced_tab = AdvancedTab::Main;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_advanced_contest_tab() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::Advanced;
        app.form.advanced_tab = AdvancedTab::Contest;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_advanced_technical_tab() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::Advanced;
        app.form.advanced_tab = AdvancedTab::Technical;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_advanced_awards_tab() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::Advanced;
        app.form.advanced_tab = AdvancedTab::Awards;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_help_view() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::Help;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_confirm_delete_view_no_candidate() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::ConfirmDeleteQso;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_confirm_delete_view_with_candidate() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("del-id", "K7ABC"));
        app.view = View::ConfirmDeleteQso;
        app.delete_candidate_id = Some("del-id".to_string());
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_status_message() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.set_status("QSO logged: K7ABC");
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_error_message() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.set_error("Connection refused");
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_space_weather_data() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.space_weather = Some(SpaceWeatherInfo {
            k_index: Some(3.5),
            solar_flux: Some(145.2),
            sunspot_number: Some(87),
        });
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_without_space_weather() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.space_weather = None;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_recent_qsos_and_selection() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        app.recent_qsos.push(make_qso("2", "W1XYZ"));
        app.qso_selected = Some(0);
        app.qso_list_focused = true;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_search_focused() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.search_focused = true;
        app.search_text = "K7".to_string();
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_lookup_result() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.lookup_result = Some(CallsignInfo {
            callsign: "K7ABC".to_string(),
            name: Some("John Smith".to_string()),
            qth: Some("Seattle WA".to_string()),
            grid: Some("CN87".to_string()),
            country: Some("United States".to_string()),
            cq_zone: Some(3),
            dxcc: Some(291),
        });
        app.form.callsign = "K7ABC".to_string();
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_lookup_result_no_optional_fields() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.lookup_result = Some(CallsignInfo {
            callsign: "K7ABC".to_string(),
            name: None,
            qth: None,
            grid: None,
            country: None,
            cq_zone: None,
            dxcc: None,
        });
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_advanced_with_focused_callsign_selected() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.view = View::Advanced;
        app.form.advanced_tab = AdvancedTab::Main;
        app.form.field_selected = true;
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_with_qso_timer_active() {
        let mut terminal = make_terminal();
        let mut app = make_app();
        app.qso_timer_active = true;
        app.form.callsign = "K7ABC".to_string();
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }

    #[test]
    fn render_small_terminal() {
        let backend = TestBackend::new(40, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let app = make_app();
        terminal.draw(|f| super::render_ui(&app, f)).unwrap();
    }
}
