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
    if editing_id.is_none() && form_snap.time_off.is_empty() {
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
