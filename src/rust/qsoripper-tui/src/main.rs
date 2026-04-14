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
use form::{Field, LogForm, BANDS, MODES};

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
            let now = chrono::Utc::now();
            app.form.time = now.format("%H:%M").to_string();
        }
        AppEvent::LookupResult(result) => {
            app.lookup_result = result;
        }
        AppEvent::SpaceWeather(sw) => {
            app.space_weather = sw;
        }
        AppEvent::QsoLogged(local_id) => {
            app.set_status(format!("QSO logged: {local_id}"));
            app.form = LogForm::new();
            app.lookup_result = None;
            refresh_recent_qsos(event_tx, endpoint);
        }
        AppEvent::QsoLogFailed(err) => {
            app.set_error(format!("Log failed: {err}"));
        }
        AppEvent::RecentQsos(qsos) => {
            app.recent_qsos = qsos;
        }
    }
}

/// Handle a key event in the current view.
fn handle_key(
    app: &mut App,
    key: crossterm::event::KeyEvent,
    event_tx: &mpsc::UnboundedSender<AppEvent>,
    lookup_tx: &watch::Sender<String>,
    endpoint: &str,
) {
    use crossterm::event::{KeyCode, KeyModifiers};

    // Any key closes the help overlay.
    if matches!(app.view, app::View::Help) {
        app.view = app::View::LogEntry;
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
            }
            app::View::LogEntry => {
                app.view = app::View::Advanced;
                app.form.focused = Field::TxPower;
            }
            app::View::Help => {}
        },
        KeyCode::F(10) => {
            let tx = event_tx.clone();
            let ep = endpoint.to_string();
            let form_snap = app.form.clone();
            tokio::spawn(async move {
                match grpc::create_channel(&ep).await {
                    Ok(ch) => match grpc::log_qso(ch, &form_snap).await {
                        Ok(id) => {
                            let _ = tx.send(AppEvent::QsoLogged(id));
                        }
                        Err(e) => {
                            let _ = tx.send(AppEvent::QsoLogFailed(e.to_string()));
                        }
                    },
                    Err(e) => {
                        let _ = tx.send(AppEvent::QsoLogFailed(e.to_string()));
                    }
                }
            });
        }
        KeyCode::Esc => match app.view {
            app::View::Advanced => {
                app.view = app::View::LogEntry;
                app.form.focused = Field::Callsign;
            }
            app::View::LogEntry => {
                app.form = LogForm::new();
                app.lookup_result = None;
            }
            app::View::Help => {}
        },
        KeyCode::Char('q' | 'Q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.running = false;
        }
        KeyCode::Left if app.form.is_cycle_field() => {
            cycle_left(app);
        }
        KeyCode::Right if app.form.is_cycle_field() => {
            cycle_right(app);
        }
        KeyCode::Backspace => {
            let focused = app.form.focused.clone();
            if let Some(text) = app.form.current_field_text_mut() {
                text.pop();
            }
            if focused == Field::Callsign {
                let callsign = app.form.callsign.clone();
                let _ = lookup_tx.send(callsign);
            }
        }
        KeyCode::Char(c) => {
            let focused = app.form.focused.clone();
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
        _ => {}
    }
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
