//! Binary entry point for the `QsoRipper` terminal UI.

mod app;
mod events;
mod form;
mod grpc;
mod ui;

use std::io;

use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use tokio::sync::{mpsc, watch};

use app::App;
use events::{spawn_clock_task, spawn_key_task, spawn_lookup_task, AppEvent};
use form::{AdvancedTab, Field, LogForm, BANDS, MODES};

/// Application entry point.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = parse_endpoint_arg();

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, endpoint).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

/// Parse `--endpoint <url>` from `argv`, defaulting to `http://127.0.0.1:50051`.
fn parse_endpoint_arg() -> String {
    let args: Vec<String> = std::env::args().collect();
    let mut endpoint = "http://127.0.0.1:50051".to_string();
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--endpoint" {
            if let Some(next) = iter.next() {
                endpoint.clone_from(next);
            }
        }
    }
    endpoint
}

/// Main run loop — creates the app, spawns background tasks, and drives the event loop.
async fn run<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    endpoint: String,
) -> anyhow::Result<()> {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AppEvent>();
    let (lookup_tx, lookup_rx) = watch::channel(String::new());

    let mut app = App::new(endpoint);

    spawn_key_task(event_tx.clone());
    spawn_clock_task(event_tx.clone());
    spawn_lookup_task(lookup_rx, event_tx.clone(), app.endpoint.clone());

    // Prefetch space weather and recent QSOs on startup.
    {
        let tx = event_tx.clone();
        let ep = app.endpoint.clone();
        tokio::spawn(async move {
            if let Ok(ch) = grpc::create_channel(&ep).await {
                if let Ok(sw) = grpc::get_space_weather(ch).await {
                    let _ = tx.send(AppEvent::SpaceWeather(sw));
                }
            }
        });
    }
    {
        let tx = event_tx.clone();
        let ep = app.endpoint.clone();
        tokio::spawn(async move {
            if let Ok(ch) = grpc::create_channel(&ep).await {
                if let Ok(qsos) = grpc::list_recent_qsos(ch, 20).await {
                    let _ = tx.send(AppEvent::RecentQsos(qsos));
                }
            }
        });
    }

    terminal.draw(|f| ui::render_ui(&app, f))?;

    while app.running {
        if let Some(event) = event_rx.recv().await {
            let endpoint = app.endpoint.clone();
            handle_event(&mut app, event, &event_tx, &lookup_tx, &endpoint);
            app.expire_status();
            terminal.draw(|f| ui::render_ui(&app, f))?;
        }
    }

    Ok(())
}

/// Dispatch a single [`AppEvent`] to the appropriate handler.
fn handle_event(
    app: &mut App,
    event: AppEvent,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    lookup_tx: &watch::Sender<String>,
    endpoint: &str,
) {
    match event {
        AppEvent::Key(key) => handle_key(app, key, event_tx, lookup_tx, endpoint),
        AppEvent::Tick => {
            app.utc_now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
            app.tick_debounce();
        }
        AppEvent::LookupResult(result) => {
            if let Some(ref info) = result {
                if app.form.qth.is_empty() {
                    if let Some(ref qth) = info.qth {
                        app.form.qth.clone_from(qth);
                    }
                }
                if app.form.worked_name.is_empty() {
                    if let Some(ref name) = info.name {
                        app.form.worked_name.clone_from(name);
                    }
                }
            }
            app.lookup_result = result;
        }
        AppEvent::SpaceWeather(sw) => {
            app.space_weather = sw;
        }
        AppEvent::QsoLogged(local_id) => {
            app.set_status(format!("QSO logged: {local_id}"));
            let band_idx = app.form.band_idx;
            let mode_idx = app.form.mode_idx;
            app.form = LogForm::new();
            app.form.band_idx = band_idx;
            app.form.mode_idx = mode_idx;
            app.form.on_band_change();
            app.lookup_result = None;
            app.reset_timer();
            refresh_recent_qsos(event_tx, endpoint);
        }
        AppEvent::QsoLogFailed(err) => {
            app.set_error(format!("Log failed: {err}"));
        }
        AppEvent::QsoUpdated(callsign) => {
            app.set_status(format!("QSO updated: {callsign}"));
            let band_idx = app.form.band_idx;
            let mode_idx = app.form.mode_idx;
            app.form = LogForm::new();
            app.form.band_idx = band_idx;
            app.form.mode_idx = mode_idx;
            app.form.on_band_change();
            app.lookup_result = None;
            app.editing_local_id = None;
            app.reset_timer();
            refresh_recent_qsos(event_tx, endpoint);
        }
        AppEvent::QsoUpdateFailed(err) => {
            app.set_error(format!("Update failed: {err}"));
        }
        AppEvent::QsoDeleted(local_id) => {
            app.set_status(format!("QSO {local_id} deleted"));
            app.delete_candidate_id = None;
            app.view = app::View::LogEntry;
            app.qso_list_focused = false;
            app.qso_selected = None;
            refresh_recent_qsos(event_tx, endpoint);
        }
        AppEvent::QsoDeleteFailed(err) => {
            app.set_error(format!("Delete failed: {err}"));
            app.delete_candidate_id = None;
            app.view = app::View::LogEntry;
        }
        AppEvent::RecentQsos(qsos) => {
            app.recent_qsos = qsos;
            // Clamp selection to the new filtered length.
            let max = app.filtered_qsos().len().saturating_sub(1);
            if let Some(sel) = app.qso_selected {
                if sel > max {
                    app.qso_selected = if app.filtered_qsos().is_empty() {
                        None
                    } else {
                        Some(max)
                    };
                }
            }
            // Enrich QSOs that have no operator name from the lookup cache.
            let unnamed: Vec<(String, String)> = app
                .recent_qsos
                .iter()
                .filter(|q| q.name.is_none())
                .map(|q| (q.local_id.clone(), q.callsign.clone()))
                .collect();
            if !unnamed.is_empty() {
                enrich_names(unnamed, event_tx, endpoint);
            }
        }
        AppEvent::QsoNameEnriched { local_id, name } => {
            if let Some(q) = app.recent_qsos.iter_mut().find(|q| q.local_id == local_id) {
                q.name = Some(name);
            }
        }
    }
}

/// Handle a key event in the current view.
#[expect(
    clippy::too_many_lines,
    reason = "top-level key dispatch; splitting would obscure the routing logic"
)]
fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    lookup_tx: &watch::Sender<String>,
    endpoint: &str,
) {
    use crossterm::event::{KeyCode, KeyModifiers};

    // Ctrl+Q quits from any state.
    if matches!(key.code, KeyCode::Char('q' | 'Q')) && key.modifiers.contains(KeyModifiers::CONTROL)
    {
        app.running = false;
        return;
    }

    if matches!(app.view, app::View::Help) {
        app.view = app::View::LogEntry;
        return;
    }

    if matches!(app.view, app::View::ConfirmDeleteQso) {
        handle_confirm_delete_key(app, key, event_tx, endpoint);
        return;
    }

    if app.search_focused {
        handle_search_key(app, key);
        return;
    }

    if app.qso_list_focused {
        handle_qso_list_key(app, key, lookup_tx);
        return;
    }

    match key.code {
        KeyCode::Tab => match app.view {
            app::View::Advanced => app.form.next_advanced_field(),
            _ => app.form.next_field(),
        },
        KeyCode::BackTab => match app.view {
            app::View::Advanced => app.form.prev_advanced_field(),
            _ => app.form.prev_field(),
        },
        KeyCode::F(1) => app.view = app::View::Help,
        KeyCode::F(2) => match app.view {
            app::View::Advanced => {
                app.view = app::View::LogEntry;
                app.form.focused = Field::Callsign;
                app.form.field_selected = false;
            }
            app::View::LogEntry => {
                app.view = app::View::Advanced;
                app.form.advanced_tab = AdvancedTab::Main;
                app.form.focused = Field::Callsign;
                app.form.field_selected = true;
            }
            app::View::Help | app::View::ConfirmDeleteQso => {}
        },
        KeyCode::F(3) => {
            if !app.filtered_qsos().is_empty() {
                app.qso_list_focused = true;
                app.qso_selected = Some(0);
            }
        }
        KeyCode::F(4) => {
            app.search_focused = true;
            app.qso_list_focused = false;
        }
        KeyCode::F(5) if matches!(app.view, app::View::Advanced) => {
            app.form.next_advanced_tab();
        }
        KeyCode::F(6) if matches!(app.view, app::View::Advanced) => {
            app.form.prev_advanced_tab();
        }
        KeyCode::F(7) => {
            app.reset_qso_start_time();
            app.set_status(format!("QSO start time reset to {}", app.form.time));
        }
        KeyCode::F(10) => spawn_log_qso(app, event_tx, endpoint),
        KeyCode::Enter if key.modifiers.contains(KeyModifiers::ALT) => {
            spawn_log_qso(app, event_tx, endpoint);
        }
        KeyCode::End => {
            app.form.field_selected = false;
        }
        KeyCode::Esc => match app.view {
            app::View::Advanced => {
                app.view = app::View::LogEntry;
                app.form.focused = Field::Callsign;
                app.form.field_selected = false;
            }
            app::View::LogEntry => {
                app.form = LogForm::new();
                app.lookup_result = None;
                app.editing_local_id = None;
                app.reset_timer();
            }
            app::View::Help | app::View::ConfirmDeleteQso => {}
        },
        KeyCode::Left if app.form.is_cycle_field() => cycle_left(app),
        KeyCode::Right if app.form.is_cycle_field() => cycle_right(app),
        KeyCode::Backspace => {
            let focused = app.form.focused.clone();
            if app.form.field_selected {
                if let Some(text) = app.form.current_field_text_mut() {
                    text.clear();
                }
                app.form.field_selected = false;
            } else if let Some(text) = app.form.current_field_text_mut() {
                text.pop();
            }
            if focused == Field::Callsign {
                let callsign = app.form.callsign.clone();
                app.on_callsign_changed();
                let _ = lookup_tx.send(callsign);
            }
        }
        KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::ALT) => {
            jump_to_field(app, c);
        }
        KeyCode::Char(c) => handle_char_key(app, c, lookup_tx),
        _ => {}
    }
}

/// Handle keyboard input while the search box is focused.
fn handle_search_key(app: &mut App, key: crossterm::event::KeyEvent) {
    use crossterm::event::{KeyCode, KeyModifiers};
    match key.code {
        KeyCode::Esc => {
            app.search_text.clear();
            app.search_focused = false;
            app.qso_selected = None;
        }
        KeyCode::Backspace => {
            app.search_text.pop();
            app.qso_selected = None;
        }
        KeyCode::Down | KeyCode::Enter | KeyCode::F(3) => {
            app.search_focused = false;
            let has_results = !app.filtered_qsos().is_empty();
            if has_results {
                app.qso_list_focused = true;
                app.qso_selected = Some(0);
            }
        }
        KeyCode::Tab => {
            app.search_focused = false;
        }
        KeyCode::Char(c)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            app.search_text.push(c);
            app.qso_selected = None;
        }
        _ => {}
    }
}

/// Navigate the QSO list with keyboard (active when `app.qso_list_focused` is true).
fn handle_qso_list_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    lookup_tx: &watch::Sender<String>,
) {
    use crossterm::event::KeyCode;
    let (max, enter_id, delete_id) = {
        let filtered = app.filtered_qsos();
        let max = filtered.len().saturating_sub(1);
        let enter_id = app
            .qso_selected
            .and_then(|i| filtered.get(i).map(|q| q.local_id.clone()));
        let delete_id = app
            .qso_selected
            .and_then(|i| filtered.get(i).map(|q| q.local_id.clone()));
        (max, enter_id, delete_id)
    };
    match key.code {
        KeyCode::Up => {
            app.qso_selected = Some(match app.qso_selected {
                Some(i) if i > 0 => i - 1,
                _ => 0,
            });
        }
        KeyCode::Down => {
            app.qso_selected = Some(match app.qso_selected {
                Some(i) => (i + 1).min(max),
                None => 0,
            });
        }
        KeyCode::Enter => {
            if let Some(id) = enter_id {
                load_qso_into_form(app, &id, lookup_tx);
            } else {
                app.qso_list_focused = false;
            }
        }
        KeyCode::Char('d' | 'D') | KeyCode::Delete => {
            if let Some(id) = delete_id {
                app.delete_candidate_id = Some(id);
                app.view = app::View::ConfirmDeleteQso;
            }
        }
        KeyCode::Esc | KeyCode::F(3) => {
            app.qso_list_focused = false;
            app.qso_selected = None;
        }
        _ => {}
    }
}

/// Handle key input while the delete-confirmation dialog is showing.
fn handle_confirm_delete_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    endpoint: &str,
) {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Char('y' | 'Y') | KeyCode::Enter => {
            if let Some(ref id) = app.delete_candidate_id.clone() {
                spawn_delete_qso(id, event_tx, endpoint);
            }
        }
        KeyCode::Char('n' | 'N') | KeyCode::Esc => {
            app.delete_candidate_id = None;
            app.view = app::View::LogEntry;
        }
        _ => {}
    }
}

/// Spawn a task to delete a QSO by its local ID and forward the result to the event channel.
fn spawn_delete_qso(local_id: &str, event_tx: &mpsc::UnboundedSender<AppEvent>, endpoint: &str) {
    let tx = event_tx.clone();
    let ep = endpoint.to_string();
    let id = local_id.to_string();
    tokio::spawn(async move {
        match grpc::create_channel(&ep).await {
            Ok(ch) => match grpc::delete_qso(ch, &id).await {
                Ok(()) => {
                    let _ = tx.send(AppEvent::QsoDeleted(id));
                }
                Err(e) => {
                    let _ = tx.send(AppEvent::QsoDeleteFailed(e.to_string()));
                }
            },
            Err(e) => {
                let _ = tx.send(AppEvent::QsoDeleteFailed(e.to_string()));
            }
        }
    });
}

/// Handle a plain character key press — type-selects Band/Mode, or appends to text fields.
fn handle_char_key(app: &mut App, c: char, lookup_tx: &watch::Sender<String>) {
    let focused = app.form.focused.clone();
    match focused {
        Field::Band => app.form.type_select_band(c),
        Field::Mode => app.form.type_select_mode(c),
        _ => {
            if app.form.field_selected {
                if let Some(text) = app.form.current_field_text_mut() {
                    text.clear();
                }
                app.form.field_selected = false;
            }
            if let Some(text) = app.form.current_field_text_mut() {
                if focused == Field::Callsign {
                    text.push(c.to_ascii_uppercase());
                } else {
                    text.push(c);
                }
            }
            if focused == Field::Callsign {
                app.on_callsign_changed();
                let callsign = app.form.callsign.clone();
                let _ = lookup_tx.send(callsign);
            }
        }
    }
}

/// Jump form focus to the field bound to `ch` (Alt+key mapping).
fn jump_to_field(app: &mut App, ch: char) {
    let target = match ch.to_ascii_lowercase() {
        'c' => Field::Callsign,
        'b' => Field::Band,
        'm' => Field::Mode,
        's' => Field::RstSent,
        'r' => Field::RstRcvd,
        'o' => Field::Comment,
        'n' => Field::Notes,
        'f' => Field::FrequencyMhz,
        'd' => Field::Date,
        't' => Field::Time,
        'e' => Field::TimeOff,
        'q' => Field::Qth,
        'a' => Field::WorkedName,
        _ => return,
    };
    if !matches!(app.view, app::View::Advanced) && matches!(target, Field::WorkedName) {
        app.view = app::View::Advanced;
        app.form.advanced_tab = AdvancedTab::Main;
    }
    app.form.focused = target;
    app.form.field_selected = true;
    app.qso_list_focused = false;
}

/// Load the QSO identified by `local_id` into the form for editing.
///
/// Sets `editing_local_id` so that saving the form calls `UpdateQso` instead of `LogQso`.
fn load_qso_into_form(app: &mut App, local_id: &str, lookup_tx: &watch::Sender<String>) {
    let data = app
        .recent_qsos
        .iter()
        .find(|q| q.local_id == local_id)
        .map(|q| {
            (
                q.callsign.clone(),
                q.band.clone(),
                q.mode.clone(),
                q.rst_sent.clone(),
                q.rst_rcvd.clone(),
                q.utc.clone(),
                q.name.clone(),
            )
        });
    let Some((callsign, band, mode, rst_sent, rst_rcvd, utc, name)) = data else {
        return;
    };
    app.form.callsign = callsign;
    if let Some(bi) = BANDS.iter().position(|&b| b == band.as_str()) {
        app.form.band_idx = bi;
    }
    if let Some(mi) = MODES.iter().position(|&m| m == mode.as_str()) {
        app.form.mode_idx = mi;
    }
    app.form.on_band_change();
    app.form.rst_sent = rst_sent;
    app.form.rst_rcvd = rst_rcvd;
    app.form.time = utc;
    app.form.worked_name = name.unwrap_or_default();
    app.form.focused = Field::Callsign;
    app.form.field_selected = true;
    app.qso_list_focused = false;
    app.qso_selected = None;
    app.editing_local_id = Some(local_id.to_string());
    if matches!(app.view, app::View::Advanced) {
        app.view = app::View::LogEntry;
    }
    let _ = lookup_tx.send(app.form.callsign.clone());
}

/// Spawn a task to save the current form contents.
///
/// If `editing_local_id` is set, calls `UpdateQso`; otherwise calls `LogQso`.
fn spawn_log_qso(app: &App, event_tx: &mpsc::UnboundedSender<AppEvent>, endpoint: &str) {
    let tx = event_tx.clone();
    let ep = endpoint.to_string();
    let mut form_snap = app.form.clone();
    let editing_id = app.editing_local_id.clone();
    if editing_id.is_none() {
        form_snap.time_off = chrono::Utc::now().format("%H:%M").to_string();
    }
    let lookup_snap = app.lookup_result.as_ref().map(|info| {
        (
            info.grid.clone(),
            info.country.clone(),
            info.cq_zone,
            info.dxcc,
        )
    });
    tokio::spawn(async move {
        match grpc::create_channel(&ep).await {
            Ok(ch) => {
                if let Some(local_id) = editing_id {
                    match grpc::update_qso(ch, &local_id, &form_snap, lookup_snap).await {
                        Ok(()) => {
                            let callsign = form_snap.callsign.to_uppercase();
                            let _ = tx.send(AppEvent::QsoUpdated(callsign));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::QsoUpdateFailed(e.to_string()));
                        }
                    }
                } else {
                    match grpc::log_qso(ch, &form_snap, lookup_snap).await {
                        Ok(id) => {
                            let _ = tx.send(AppEvent::QsoLogged(id));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::QsoLogFailed(e.to_string()));
                        }
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(AppEvent::QsoLogFailed(e.to_string()));
            }
        }
    });
}

/// Cycle the focused selector one step to the left (decreasing index).
fn cycle_left(app: &mut App) {
    match app.form.focused {
        Field::Band => {
            app.form.band_idx = if app.form.band_idx == 0 {
                BANDS.len().saturating_sub(1)
            } else {
                app.form.band_idx - 1
            };
            app.form.on_band_change();
        }
        Field::Mode => {
            app.form.mode_idx = if app.form.mode_idx == 0 {
                MODES.len().saturating_sub(1)
            } else {
                app.form.mode_idx - 1
            };
            app.form.on_mode_change();
        }
        _ => {}
    }
}

/// Cycle the focused selector one step to the right (increasing index).
fn cycle_right(app: &mut App) {
    match app.form.focused {
        Field::Band => {
            app.form.band_idx = (app.form.band_idx + 1) % BANDS.len();
            app.form.on_band_change();
        }
        Field::Mode => {
            app.form.mode_idx = (app.form.mode_idx + 1) % MODES.len();
            app.form.on_mode_change();
        }
        _ => {}
    }
}

/// Spawn background tasks to enrich QSOs with operator names from the lookup cache.
///
/// For each `(local_id, callsign)` pair, performs a cache-first lookup and sends
/// [`AppEvent::QsoNameEnriched`] if a name is resolved.
fn enrich_names(
    qsos: Vec<(String, String)>,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    endpoint: &str,
) {
    let tx = event_tx.clone();
    let ep = endpoint.to_string();
    tokio::spawn(async move {
        let Ok(ch) = grpc::create_channel(&ep).await else {
            return;
        };
        for (local_id, callsign) in qsos {
            if let Ok(Some(info)) = grpc::lookup_callsign(ch.clone(), &callsign).await {
                if let Some(name) = info.name {
                    let _ = tx.send(AppEvent::QsoNameEnriched { local_id, name });
                }
            }
        }
    });
}

/// Spawn a task to refresh the recent QSOs list and forward the result to the event channel.
fn refresh_recent_qsos(event_tx: &mpsc::UnboundedSender<AppEvent>, endpoint: &str) {
    let tx = event_tx.clone();
    let ep = endpoint.to_string();
    tokio::spawn(async move {
        if let Ok(ch) = grpc::create_channel(&ep).await {
            if let Ok(qsos) = grpc::list_recent_qsos(ch, 20).await {
                let _ = tx.send(AppEvent::RecentQsos(qsos));
            }
        }
    });
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::unchecked_duration_subtraction,
    clippy::items_after_statements
)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use tokio::sync::{mpsc, watch};

    use super::*;
    use crate::app::{App, RecentQso, View};
    use crate::events::AppEvent;
    use crate::form::Field;

    fn make_key(code: KeyCode) -> KeyEvent {
        KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_key_with_mod(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn make_app() -> App {
        App::new("http://localhost:50051".to_string())
    }

    fn make_watch() -> (watch::Sender<String>, watch::Receiver<String>) {
        watch::channel(String::new())
    }

    fn make_qso(id: &str, callsign: &str) -> RecentQso {
        RecentQso {
            local_id: id.to_string(),
            utc: "12:00".to_string(),
            callsign: callsign.to_string(),
            band: "20M".to_string(),
            mode: "SSB".to_string(),
            rst_sent: "59".to_string(),
            rst_rcvd: "59".to_string(),
            country: None,
            grid: None,
            name: None,
        }
    }

    #[test]
    fn parse_endpoint_arg_defaults() {
        let ep = parse_endpoint_arg();
        assert_eq!(ep, "http://127.0.0.1:50051");
    }

    #[test]
    fn jump_to_field_callsign() {
        let mut app = make_app();
        jump_to_field(&mut app, 'c');
        assert_eq!(app.form.focused, Field::Callsign);
        assert!(app.form.field_selected);
    }

    #[test]
    fn jump_to_field_band() {
        let mut app = make_app();
        jump_to_field(&mut app, 'b');
        assert_eq!(app.form.focused, Field::Band);
    }

    #[test]
    fn jump_to_field_mode() {
        let mut app = make_app();
        jump_to_field(&mut app, 'm');
        assert_eq!(app.form.focused, Field::Mode);
    }

    #[test]
    fn jump_to_field_rst_sent() {
        let mut app = make_app();
        jump_to_field(&mut app, 's');
        assert_eq!(app.form.focused, Field::RstSent);
    }

    #[test]
    fn jump_to_field_rst_rcvd() {
        let mut app = make_app();
        jump_to_field(&mut app, 'r');
        assert_eq!(app.form.focused, Field::RstRcvd);
    }

    #[test]
    fn jump_to_field_comment() {
        let mut app = make_app();
        jump_to_field(&mut app, 'o');
        assert_eq!(app.form.focused, Field::Comment);
    }

    #[test]
    fn jump_to_field_notes() {
        let mut app = make_app();
        jump_to_field(&mut app, 'n');
        assert_eq!(app.form.focused, Field::Notes);
    }

    #[test]
    fn jump_to_field_frequency() {
        let mut app = make_app();
        jump_to_field(&mut app, 'f');
        assert_eq!(app.form.focused, Field::FrequencyMhz);
    }

    #[test]
    fn jump_to_field_date() {
        let mut app = make_app();
        jump_to_field(&mut app, 'd');
        assert_eq!(app.form.focused, Field::Date);
    }

    #[test]
    fn jump_to_field_time() {
        let mut app = make_app();
        jump_to_field(&mut app, 't');
        assert_eq!(app.form.focused, Field::Time);
    }

    #[test]
    fn jump_to_field_time_off() {
        let mut app = make_app();
        jump_to_field(&mut app, 'e');
        assert_eq!(app.form.focused, Field::TimeOff);
    }

    #[test]
    fn jump_to_field_qth() {
        let mut app = make_app();
        jump_to_field(&mut app, 'q');
        assert_eq!(app.form.focused, Field::Qth);
    }

    #[test]
    fn jump_to_field_worked_name_opens_advanced() {
        let mut app = make_app();
        assert!(matches!(app.view, View::LogEntry));
        jump_to_field(&mut app, 'a');
        assert_eq!(app.form.focused, Field::WorkedName);
        assert!(matches!(app.view, View::Advanced));
    }

    #[test]
    fn jump_to_field_worked_name_stays_advanced_when_already_advanced() {
        let mut app = make_app();
        app.view = View::Advanced;
        jump_to_field(&mut app, 'a');
        assert_eq!(app.form.focused, Field::WorkedName);
        assert!(matches!(app.view, View::Advanced));
    }

    #[test]
    fn jump_to_field_unknown_char_does_nothing() {
        let mut app = make_app();
        let original_focused = app.form.focused.clone();
        jump_to_field(&mut app, 'z');
        assert_eq!(app.form.focused, original_focused);
    }

    #[test]
    fn cycle_left_band_decrements() {
        let mut app = make_app();
        app.form.focused = Field::Band;
        app.form.band_idx = 5;
        cycle_left(&mut app);
        assert_eq!(app.form.band_idx, 4);
    }

    #[test]
    fn cycle_left_band_wraps_to_last() {
        let mut app = make_app();
        app.form.focused = Field::Band;
        app.form.band_idx = 0;
        cycle_left(&mut app);
        assert_eq!(app.form.band_idx, BANDS.len() - 1);
    }

    #[test]
    fn cycle_right_band_increments() {
        let mut app = make_app();
        app.form.focused = Field::Band;
        app.form.band_idx = 3;
        cycle_right(&mut app);
        assert_eq!(app.form.band_idx, 4);
    }

    #[test]
    fn cycle_right_band_wraps_to_zero() {
        let mut app = make_app();
        app.form.focused = Field::Band;
        app.form.band_idx = BANDS.len() - 1;
        cycle_right(&mut app);
        assert_eq!(app.form.band_idx, 0);
    }

    #[test]
    fn cycle_left_mode_decrements() {
        let mut app = make_app();
        app.form.focused = Field::Mode;
        app.form.mode_idx = 2;
        cycle_left(&mut app);
        assert_eq!(app.form.mode_idx, 1);
    }

    #[test]
    fn cycle_left_mode_wraps_to_last() {
        let mut app = make_app();
        app.form.focused = Field::Mode;
        app.form.mode_idx = 0;
        cycle_left(&mut app);
        assert_eq!(app.form.mode_idx, MODES.len() - 1);
    }

    #[test]
    fn cycle_right_mode_wraps_to_zero() {
        let mut app = make_app();
        app.form.focused = Field::Mode;
        app.form.mode_idx = MODES.len() - 1;
        cycle_right(&mut app);
        assert_eq!(app.form.mode_idx, 0);
    }

    #[test]
    fn cycle_left_on_non_cycle_field_does_nothing() {
        let mut app = make_app();
        app.form.focused = Field::Callsign;
        let before = app.form.band_idx;
        cycle_left(&mut app);
        assert_eq!(app.form.band_idx, before);
    }

    #[test]
    fn cycle_right_on_non_cycle_field_does_nothing() {
        let mut app = make_app();
        app.form.focused = Field::Callsign;
        let before = app.form.mode_idx;
        cycle_right(&mut app);
        assert_eq!(app.form.mode_idx, before);
    }

    #[test]
    fn handle_search_key_esc_clears_and_unfocuses() {
        let mut app = make_app();
        app.search_focused = true;
        app.search_text = "K7".to_string();
        app.qso_selected = Some(0);
        handle_search_key(&mut app, make_key(KeyCode::Esc));
        assert!(app.search_text.is_empty());
        assert!(!app.search_focused);
        assert!(app.qso_selected.is_none());
    }

    #[test]
    fn handle_search_key_backspace_pops_char() {
        let mut app = make_app();
        app.search_focused = true;
        app.search_text = "K7A".to_string();
        handle_search_key(&mut app, make_key(KeyCode::Backspace));
        assert_eq!(app.search_text, "K7");
        assert!(app.qso_selected.is_none());
    }

    #[test]
    fn handle_search_key_char_appends() {
        let mut app = make_app();
        app.search_focused = true;
        handle_search_key(&mut app, make_key(KeyCode::Char('K')));
        assert_eq!(app.search_text, "K");
    }

    #[test]
    fn handle_search_key_ctrl_char_ignored() {
        let mut app = make_app();
        app.search_focused = true;
        handle_search_key(
            &mut app,
            make_key_with_mod(KeyCode::Char('c'), KeyModifiers::CONTROL),
        );
        assert!(app.search_text.is_empty());
    }

    #[test]
    fn handle_search_key_tab_unfocuses() {
        let mut app = make_app();
        app.search_focused = true;
        handle_search_key(&mut app, make_key(KeyCode::Tab));
        assert!(!app.search_focused);
    }

    #[test]
    fn handle_search_key_enter_enters_list_when_results_exist() {
        let mut app = make_app();
        app.search_focused = true;
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        handle_search_key(&mut app, make_key(KeyCode::Enter));
        assert!(!app.search_focused);
        assert!(app.qso_list_focused);
        assert_eq!(app.qso_selected, Some(0));
    }

    #[test]
    fn handle_search_key_enter_no_results_stays_unfocused() {
        let mut app = make_app();
        app.search_focused = true;
        app.search_text = "zzzz".to_string();
        handle_search_key(&mut app, make_key(KeyCode::Enter));
        assert!(!app.qso_list_focused);
    }

    #[test]
    fn handle_search_key_down_enters_list() {
        let mut app = make_app();
        app.search_focused = true;
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        handle_search_key(&mut app, make_key(KeyCode::Down));
        assert!(app.qso_list_focused);
    }

    #[test]
    fn handle_search_key_f3_enters_list() {
        let mut app = make_app();
        app.search_focused = true;
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        handle_search_key(&mut app, make_key(KeyCode::F(3)));
        assert!(!app.search_focused);
    }

    #[test]
    fn handle_qso_list_key_esc_unfocuses() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.qso_list_focused = true;
        app.qso_selected = Some(1);
        handle_qso_list_key(&mut app, make_key(KeyCode::Esc), &lookup_tx);
        assert!(!app.qso_list_focused);
        assert!(app.qso_selected.is_none());
    }

    #[test]
    fn handle_qso_list_key_f3_unfocuses() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.qso_list_focused = true;
        handle_qso_list_key(&mut app, make_key(KeyCode::F(3)), &lookup_tx);
        assert!(!app.qso_list_focused);
    }

    #[test]
    fn handle_qso_list_key_up_decrements_selection() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        app.recent_qsos.push(make_qso("2", "W1XYZ"));
        app.qso_list_focused = true;
        app.qso_selected = Some(1);
        handle_qso_list_key(&mut app, make_key(KeyCode::Up), &lookup_tx);
        assert_eq!(app.qso_selected, Some(0));
    }

    #[test]
    fn handle_qso_list_key_up_at_zero_stays_at_zero() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        app.qso_list_focused = true;
        app.qso_selected = Some(0);
        handle_qso_list_key(&mut app, make_key(KeyCode::Up), &lookup_tx);
        assert_eq!(app.qso_selected, Some(0));
    }

    #[test]
    fn handle_qso_list_key_down_increments_selection() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        app.recent_qsos.push(make_qso("2", "W1XYZ"));
        app.qso_list_focused = true;
        app.qso_selected = Some(0);
        handle_qso_list_key(&mut app, make_key(KeyCode::Down), &lookup_tx);
        assert_eq!(app.qso_selected, Some(1));
    }

    #[test]
    fn handle_qso_list_key_down_clamps_at_max() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        app.qso_list_focused = true;
        app.qso_selected = Some(0);
        handle_qso_list_key(&mut app, make_key(KeyCode::Down), &lookup_tx);
        assert_eq!(app.qso_selected, Some(0));
    }

    #[test]
    fn handle_qso_list_key_delete_sets_confirm_view() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("del-1", "K7ABC"));
        app.qso_list_focused = true;
        app.qso_selected = Some(0);
        handle_qso_list_key(&mut app, make_key(KeyCode::Delete), &lookup_tx);
        assert!(matches!(app.view, View::ConfirmDeleteQso));
        assert_eq!(app.delete_candidate_id, Some("del-1".to_string()));
    }

    #[test]
    fn handle_qso_list_key_d_sets_confirm_view() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("del-2", "W1XYZ"));
        app.qso_list_focused = true;
        app.qso_selected = Some(0);
        handle_qso_list_key(&mut app, make_key(KeyCode::Char('d')), &lookup_tx);
        assert!(matches!(app.view, View::ConfirmDeleteQso));
    }

    #[test]
    fn handle_qso_list_key_enter_no_selection_unfocuses() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.qso_list_focused = true;
        app.qso_selected = None;
        handle_qso_list_key(&mut app, make_key(KeyCode::Enter), &lookup_tx);
        assert!(!app.qso_list_focused);
    }

    #[test]
    fn load_qso_into_form_populates_fields() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        let qso = RecentQso {
            local_id: "q1".to_string(),
            utc: "14:32".to_string(),
            callsign: "K7ABC".to_string(),
            band: "40M".to_string(),
            mode: "CW".to_string(),
            rst_sent: "599".to_string(),
            rst_rcvd: "599".to_string(),
            country: None,
            grid: None,
            name: Some("John".to_string()),
        };
        app.recent_qsos.push(qso);
        load_qso_into_form(&mut app, "q1", &lookup_tx);
        assert_eq!(app.form.callsign, "K7ABC");
        assert_eq!(app.form.band_str(), "40M");
        assert_eq!(app.form.mode_str(), "CW");
        assert_eq!(app.form.rst_sent, "599");
        assert_eq!(app.editing_local_id, Some("q1".to_string()));
        assert!(!app.qso_list_focused);
    }

    #[test]
    fn load_qso_into_form_unknown_id_does_nothing() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        load_qso_into_form(&mut app, "nonexistent", &lookup_tx);
        assert!(app.editing_local_id.is_none());
        assert!(app.form.callsign.is_empty());
    }

    #[test]
    fn load_qso_into_form_switches_from_advanced_to_log_entry() {
        let (lookup_tx, _rx) = make_watch();
        let mut app = make_app();
        app.view = View::Advanced;
        app.recent_qsos.push(make_qso("q1", "W1ABC"));
        load_qso_into_form(&mut app, "q1", &lookup_tx);
        assert!(matches!(app.view, View::LogEntry));
    }

    #[tokio::test]
    async fn handle_event_tick_updates_utc() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        let original_utc = app.utc_now.clone();
        handle_event(
            &mut app,
            AppEvent::Tick,
            &tx,
            &lookup_tx,
            "http://localhost:50051",
        );
        assert_ne!(app.utc_now, "");
        let _ = original_utc;
    }

    #[tokio::test]
    async fn handle_event_space_weather_updates() {
        use crate::app::SpaceWeatherInfo;
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        let sw = SpaceWeatherInfo {
            k_index: Some(2.0),
            solar_flux: Some(130.0),
            sunspot_number: Some(50),
        };
        handle_event(
            &mut app,
            AppEvent::SpaceWeather(Some(sw)),
            &tx,
            &lookup_tx,
            "",
        );
        assert!(app.space_weather.is_some());
    }

    #[tokio::test]
    async fn handle_event_lookup_result_populates_empty_qth() {
        use crate::app::CallsignInfo;
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        let info = CallsignInfo {
            callsign: "K7ABC".to_string(),
            name: Some("John".to_string()),
            qth: Some("Seattle".to_string()),
            grid: None,
            country: None,
            cq_zone: None,
            dxcc: None,
        };
        handle_event(
            &mut app,
            AppEvent::LookupResult(Some(info)),
            &tx,
            &lookup_tx,
            "",
        );
        assert_eq!(app.form.qth, "Seattle");
        assert_eq!(app.form.worked_name, "John");
    }

    #[tokio::test]
    async fn handle_event_lookup_result_does_not_override_filled_qth() {
        use crate::app::CallsignInfo;
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.qth = "Portland".to_string();
        let info = CallsignInfo {
            callsign: "K7ABC".to_string(),
            name: None,
            qth: Some("Seattle".to_string()),
            grid: None,
            country: None,
            cq_zone: None,
            dxcc: None,
        };
        handle_event(
            &mut app,
            AppEvent::LookupResult(Some(info)),
            &tx,
            &lookup_tx,
            "",
        );
        assert_eq!(app.form.qth, "Portland");
    }

    #[tokio::test]
    async fn handle_event_lookup_result_none_clears_result() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_event(&mut app, AppEvent::LookupResult(None), &tx, &lookup_tx, "");
        assert!(app.lookup_result.is_none());
    }

    #[tokio::test]
    async fn handle_event_qso_log_failed_sets_error() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_event(
            &mut app,
            AppEvent::QsoLogFailed("timeout".to_string()),
            &tx,
            &lookup_tx,
            "",
        );
        let msg = app.status_message.as_ref().unwrap();
        assert!(msg.is_error);
        assert!(msg.text.contains("timeout"));
    }

    #[tokio::test]
    async fn handle_event_qso_update_failed_sets_error() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_event(
            &mut app,
            AppEvent::QsoUpdateFailed("server error".to_string()),
            &tx,
            &lookup_tx,
            "",
        );
        let msg = app.status_message.as_ref().unwrap();
        assert!(msg.is_error);
    }

    #[tokio::test]
    async fn handle_event_qso_delete_failed_sets_error() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_event(
            &mut app,
            AppEvent::QsoDeleteFailed("not found".to_string()),
            &tx,
            &lookup_tx,
            "",
        );
        let msg = app.status_message.as_ref().unwrap();
        assert!(msg.is_error);
        assert!(matches!(app.view, View::LogEntry));
        assert!(app.delete_candidate_id.is_none());
    }

    #[tokio::test]
    async fn handle_event_qso_logged_resets_form() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.callsign = "K7ABC".to_string();
        handle_event(
            &mut app,
            AppEvent::QsoLogged("local-id-1".to_string()),
            &tx,
            &lookup_tx,
            "",
        );
        assert!(app.form.callsign.is_empty());
        assert!(app.status_message.is_some());
        assert!(!app.status_message.as_ref().unwrap().is_error);
    }

    #[tokio::test]
    async fn handle_event_qso_updated_clears_editing_id() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.editing_local_id = Some("q1".to_string());
        handle_event(
            &mut app,
            AppEvent::QsoUpdated("K7ABC".to_string()),
            &tx,
            &lookup_tx,
            "",
        );
        assert!(app.editing_local_id.is_none());
        assert!(app.status_message.is_some());
    }

    #[tokio::test]
    async fn handle_event_qso_deleted_clears_state() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.delete_candidate_id = Some("q1".to_string());
        app.view = View::ConfirmDeleteQso;
        handle_event(
            &mut app,
            AppEvent::QsoDeleted("q1".to_string()),
            &tx,
            &lookup_tx,
            "",
        );
        assert!(app.delete_candidate_id.is_none());
        assert!(matches!(app.view, View::LogEntry));
        assert!(app.status_message.is_some());
    }

    #[tokio::test]
    async fn handle_event_recent_qsos_clamps_selection() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.qso_selected = Some(5);
        let qsos = vec![make_qso("1", "K7ABC"), make_qso("2", "W1XYZ")];
        handle_event(&mut app, AppEvent::RecentQsos(qsos), &tx, &lookup_tx, "");
        assert_eq!(app.qso_selected, Some(1));
    }

    #[tokio::test]
    async fn handle_event_recent_qsos_selection_cleared_when_empty() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.qso_selected = Some(2);
        handle_event(&mut app, AppEvent::RecentQsos(vec![]), &tx, &lookup_tx, "");
        assert!(app.qso_selected.is_none());
    }

    #[tokio::test]
    async fn handle_event_qso_name_enriched_updates_name() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("q1", "K7ABC"));
        handle_event(
            &mut app,
            AppEvent::QsoNameEnriched {
                local_id: "q1".to_string(),
                name: "John Smith".to_string(),
            },
            &tx,
            &lookup_tx,
            "",
        );
        assert_eq!(app.recent_qsos[0].name, Some("John Smith".to_string()));
    }

    #[tokio::test]
    async fn handle_key_ctrl_q_stops_app() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(
            &mut app,
            make_key_with_mod(KeyCode::Char('q'), KeyModifiers::CONTROL),
            &tx,
            &lookup_tx,
            "",
        );
        assert!(!app.running);
    }

    #[tokio::test]
    async fn handle_key_f1_shows_help() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(&mut app, make_key(KeyCode::F(1)), &tx, &lookup_tx, "");
        assert!(matches!(app.view, View::Help));
    }

    #[tokio::test]
    async fn handle_key_any_in_help_returns_to_log_entry() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.view = View::Help;
        handle_key(&mut app, make_key(KeyCode::Char('x')), &tx, &lookup_tx, "");
        assert!(matches!(app.view, View::LogEntry));
    }

    #[tokio::test]
    async fn handle_key_f2_toggles_advanced_view() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(&mut app, make_key(KeyCode::F(2)), &tx, &lookup_tx, "");
        assert!(matches!(app.view, View::Advanced));
        handle_key(&mut app, make_key(KeyCode::F(2)), &tx, &lookup_tx, "");
        assert!(matches!(app.view, View::LogEntry));
    }

    #[tokio::test]
    async fn handle_key_f3_focuses_qso_list() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.recent_qsos.push(make_qso("1", "K7ABC"));
        handle_key(&mut app, make_key(KeyCode::F(3)), &tx, &lookup_tx, "");
        assert!(app.qso_list_focused);
        assert_eq!(app.qso_selected, Some(0));
    }

    #[tokio::test]
    async fn handle_key_f4_focuses_search() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(&mut app, make_key(KeyCode::F(4)), &tx, &lookup_tx, "");
        assert!(app.search_focused);
        assert!(!app.qso_list_focused);
    }

    #[tokio::test]
    async fn handle_key_f7_resets_qso_start_time() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(&mut app, make_key(KeyCode::F(7)), &tx, &lookup_tx, "");
        assert!(app.qso_timer_active);
        assert!(app.status_message.is_some());
    }

    #[tokio::test]
    async fn handle_key_esc_clears_form_in_log_entry() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.callsign = "K7ABC".to_string();
        handle_key(&mut app, make_key(KeyCode::Esc), &tx, &lookup_tx, "");
        assert!(app.form.callsign.is_empty());
    }

    #[tokio::test]
    async fn handle_key_esc_in_advanced_returns_to_log_entry() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.view = View::Advanced;
        handle_key(&mut app, make_key(KeyCode::Esc), &tx, &lookup_tx, "");
        assert!(matches!(app.view, View::LogEntry));
    }

    #[tokio::test]
    async fn handle_key_end_deselects_field() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.field_selected = true;
        handle_key(&mut app, make_key(KeyCode::End), &tx, &lookup_tx, "");
        assert!(!app.form.field_selected);
    }

    #[tokio::test]
    async fn handle_key_tab_advances_field() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(&mut app, make_key(KeyCode::Tab), &tx, &lookup_tx, "");
        assert_eq!(app.form.focused, Field::Band);
    }

    #[tokio::test]
    async fn handle_key_backtab_goes_to_prev_field() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(&mut app, make_key(KeyCode::BackTab), &tx, &lookup_tx, "");
        assert_ne!(app.form.focused, Field::Callsign);
    }

    #[tokio::test]
    async fn handle_key_left_cycles_band() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Band;
        let before = app.form.band_idx;
        handle_key(&mut app, make_key(KeyCode::Left), &tx, &lookup_tx, "");
        assert_ne!(app.form.band_idx, before);
    }

    #[tokio::test]
    async fn handle_key_right_cycles_band() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Band;
        let before = app.form.band_idx;
        handle_key(&mut app, make_key(KeyCode::Right), &tx, &lookup_tx, "");
        assert_ne!(app.form.band_idx, before);
    }

    #[tokio::test]
    async fn handle_key_char_appends_to_callsign_uppercase() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Callsign;
        handle_key(&mut app, make_key(KeyCode::Char('k')), &tx, &lookup_tx, "");
        assert_eq!(app.form.callsign, "K");
    }

    #[tokio::test]
    async fn handle_key_char_appends_to_comment_lowercase() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Comment;
        handle_key(&mut app, make_key(KeyCode::Char('x')), &tx, &lookup_tx, "");
        assert_eq!(app.form.comment, "x");
    }

    #[tokio::test]
    async fn handle_key_char_with_field_selected_clears_first() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Comment;
        app.form.comment = "old".to_string();
        app.form.field_selected = true;
        handle_key(&mut app, make_key(KeyCode::Char('x')), &tx, &lookup_tx, "");
        assert_eq!(app.form.comment, "x");
    }

    #[tokio::test]
    async fn handle_key_backspace_pops_callsign_char() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Callsign;
        app.form.callsign = "K7A".to_string();
        handle_key(&mut app, make_key(KeyCode::Backspace), &tx, &lookup_tx, "");
        assert_eq!(app.form.callsign, "K7");
    }

    #[tokio::test]
    async fn handle_key_backspace_with_field_selected_clears_field() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.form.focused = Field::Comment;
        app.form.comment = "hello".to_string();
        app.form.field_selected = true;
        handle_key(&mut app, make_key(KeyCode::Backspace), &tx, &lookup_tx, "");
        assert!(app.form.comment.is_empty());
        assert!(!app.form.field_selected);
    }

    #[tokio::test]
    async fn handle_key_alt_char_jumps_to_field() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        handle_key(
            &mut app,
            make_key_with_mod(KeyCode::Char('f'), KeyModifiers::ALT),
            &tx,
            &lookup_tx,
            "",
        );
        assert_eq!(app.form.focused, Field::FrequencyMhz);
    }

    #[tokio::test]
    async fn handle_key_confirm_delete_n_cancels() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.view = View::ConfirmDeleteQso;
        app.delete_candidate_id = Some("q1".to_string());
        handle_key(&mut app, make_key(KeyCode::Char('n')), &tx, &lookup_tx, "");
        assert!(app.delete_candidate_id.is_none());
        assert!(matches!(app.view, View::LogEntry));
    }

    #[tokio::test]
    async fn handle_key_f5_in_advanced_switches_tab() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.view = View::Advanced;
        use crate::form::AdvancedTab;
        assert_eq!(app.form.advanced_tab, AdvancedTab::Main);
        handle_key(&mut app, make_key(KeyCode::F(5)), &tx, &lookup_tx, "");
        assert_eq!(app.form.advanced_tab, AdvancedTab::Contest);
    }

    #[tokio::test]
    async fn handle_key_f6_in_advanced_switches_tab_back() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.view = View::Advanced;
        handle_key(&mut app, make_key(KeyCode::F(6)), &tx, &lookup_tx, "");
        use crate::form::AdvancedTab;
        assert_eq!(app.form.advanced_tab, AdvancedTab::Awards);
    }

    #[tokio::test]
    async fn handle_key_search_focused_routes_to_search_handler() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.search_focused = true;
        handle_key(&mut app, make_key(KeyCode::Esc), &tx, &lookup_tx, "");
        assert!(!app.search_focused);
    }

    #[tokio::test]
    async fn handle_key_qso_list_focused_routes_to_list_handler() {
        let (tx, _rx) = mpsc::unbounded_channel::<AppEvent>();
        let (lookup_tx, _lookup_rx) = make_watch();
        let mut app = make_app();
        app.qso_list_focused = true;
        app.qso_selected = Some(0);
        handle_key(&mut app, make_key(KeyCode::Esc), &tx, &lookup_tx, "");
        assert!(!app.qso_list_focused);
    }
}
