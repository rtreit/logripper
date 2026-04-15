//! Terminal dashboard client for the `QsoRipper` stress harness.

#![expect(
    clippy::indexing_slicing,
    reason = "The dashboard uses fixed `ratatui` layout splits with known indices."
)]

use std::ffi::OsString;
use std::io;
use std::process::Stdio;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use qsoripper_core::proto::qsoripper::services::{
    stress_control_service_client::StressControlServiceClient, GetStressRunStatusRequest,
    ListStressProfilesRequest, StartStressRunRequest, StopStressRunRequest,
    StreamStressRunEventsRequest, StressLogLevel, StressProfile, StressRunSnapshot, StressRunState,
    StressVectorState,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, Cell, Clear, List, ListItem, ListState, Paragraph, Row, Table, TableState,
};
use ratatui::{Frame, Terminal};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;
use tonic::transport::Endpoint;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = parse_endpoint(std::env::args().skip(1));
    let mut managed_host = ensure_host_available(endpoint.as_str()).await?;
    let (event_sender, mut event_receiver) = mpsc::unbounded_channel();
    tokio::spawn(stream_snapshots(endpoint.clone(), event_sender));

    let profiles = list_profiles(endpoint.as_str()).await.unwrap_or_default();
    let initial_snapshot = get_status(endpoint.as_str()).await.unwrap_or_default();
    let mut app = App::new(endpoint, profiles, initial_snapshot);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run_app(&mut terminal, &mut app, &mut event_receiver).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    if let Some(host) = managed_host.as_mut() {
        stop_host(host).await;
    }

    result.map_err(Into::into)
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    event_receiver: &mut mpsc::UnboundedReceiver<AppEvent>,
) -> io::Result<()> {
    loop {
        while let Ok(event) = event_receiver.try_recv() {
            match event {
                AppEvent::Snapshot(snapshot) => app.snapshot = *snapshot,
                AppEvent::StreamError(message) => app.error_message = Some(message),
            }
        }

        app.clamp_selection();
        terminal.draw(|frame| draw(frame, app))?;

        if event::poll(Duration::from_millis(100))? {
            let Event::Key(key) = event::read()? else {
                continue;
            };
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match key.code {
                KeyCode::Char('q') => return Ok(()),
                KeyCode::Char('s') => {
                    if let Err(message) = start_selected_profile(app).await {
                        app.error_message = Some(message);
                    }
                }
                KeyCode::Char('x') => {
                    if let Err(message) = stop_run(app).await {
                        app.error_message = Some(message);
                    }
                }
                KeyCode::Char('r') => {
                    let _ = stop_run(app).await;
                    if let Err(message) = start_selected_profile(app).await {
                        app.error_message = Some(message);
                    }
                }
                KeyCode::Char('p') => app.advance_profile(),
                KeyCode::Tab => app.toggle_focus(),
                KeyCode::Up => app.move_selection(-1),
                KeyCode::Down => app.move_selection(1),
                KeyCode::Esc => app.error_message = None,
                _ => {}
            }
        }
    }
}

async fn start_selected_profile(app: &mut App) -> Result<(), String> {
    let profile_name = app
        .profiles
        .get(app.selected_profile)
        .map_or_else(|| "long-haul".to_string(), |profile| profile.name.clone());
    let mut client = connect_client(app.endpoint.as_str()).await?;
    let response = client
        .start_stress_run(StartStressRunRequest {
            profile_name,
            ..StartStressRunRequest::default()
        })
        .await
        .map_err(|error| error.to_string())?
        .into_inner();
    if let Some(snapshot) = response.snapshot {
        app.snapshot = snapshot;
    }
    app.error_message = None;
    Ok(())
}

async fn stop_run(app: &mut App) -> Result<(), String> {
    let mut client = connect_client(app.endpoint.as_str()).await?;
    let response = client
        .stop_stress_run(StopStressRunRequest { force: false })
        .await
        .map_err(|error| error.to_string())?
        .into_inner();
    if let Some(snapshot) = response.snapshot {
        app.snapshot = snapshot;
    }
    app.error_message = None;
    Ok(())
}

async fn stream_snapshots(endpoint: String, sender: mpsc::UnboundedSender<AppEvent>) {
    loop {
        let mut client = match connect_client(endpoint.as_str()).await {
            Ok(client) => client,
            Err(error) => {
                let _ = sender.send(AppEvent::StreamError(error));
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let response = match client
            .stream_stress_run_events(StreamStressRunEventsRequest {
                include_current_snapshot: true,
            })
            .await
        {
            Ok(response) => response.into_inner(),
            Err(error) => {
                let _ = sender.send(AppEvent::StreamError(format_status_error(
                    endpoint.as_str(),
                    &error,
                )));
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        let mut response = response;
        loop {
            match response.message().await {
                Ok(Some(update)) => {
                    if let Some(snapshot) = update.snapshot {
                        let _ = sender.send(AppEvent::Snapshot(Box::new(snapshot)));
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(AppEvent::StreamError(format_status_error(
                        endpoint.as_str(),
                        &error,
                    )));
                    break;
                }
            }
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

async fn list_profiles(endpoint: &str) -> Result<Vec<StressProfile>, String> {
    let mut client = connect_client(endpoint).await?;
    client
        .list_stress_profiles(ListStressProfilesRequest::default())
        .await
        .map(|response| response.into_inner().profiles)
        .map_err(|error| format_status_error(endpoint, &error))
}

async fn get_status(endpoint: &str) -> Result<StressRunSnapshot, String> {
    let mut client = connect_client(endpoint).await?;
    client
        .get_stress_run_status(GetStressRunStatusRequest::default())
        .await
        .map_err(|error| format_status_error(endpoint, &error))?
        .into_inner()
        .snapshot
        .ok_or_else(|| "Stress host returned no snapshot payload.".to_string())
}

async fn connect_client(
    endpoint: &str,
) -> Result<StressControlServiceClient<tonic::transport::Channel>, String> {
    let endpoint_address = endpoint.to_string();
    let channel_endpoint = Endpoint::from_shared(endpoint_address)
        .map_err(|error| format_connection_error(endpoint, &error.to_string()))?;
    let channel = channel_endpoint
        .connect()
        .await
        .map_err(|error| format_connection_error(endpoint, &error.to_string()))?;
    Ok(StressControlServiceClient::new(channel))
}

async fn ensure_host_available(endpoint: &str) -> Result<Option<ManagedHost>, String> {
    if can_connect(endpoint).await {
        return Ok(None);
    }

    if !is_local_endpoint(endpoint) {
        return Err(format_connection_error(
            endpoint,
            "The configured endpoint is not reachable.",
        ));
    }

    let listen = endpoint_to_listen_argument(endpoint)?;
    let mut host = start_local_host(listen.as_str())?;
    if let Err(error) = wait_for_endpoint(endpoint).await {
        stop_host(&mut host).await;
        return Err(format!(
            "Unable to auto-start a local stress host at {endpoint}.\n\n{error}\n\nTry `cargo run -p qsoripper-stress -- --listen {listen}` in another terminal."
        ));
    }

    Ok(Some(host))
}

async fn can_connect(endpoint: &str) -> bool {
    let Ok(endpoint) = Endpoint::from_shared(endpoint.to_string()) else {
        return false;
    };

    endpoint.connect().await.is_ok()
}

fn is_local_endpoint(endpoint: &str) -> bool {
    let trimmed = endpoint.trim().trim_end_matches('/');
    trimmed.starts_with("http://127.0.0.1:")
        || trimmed.starts_with("https://127.0.0.1:")
        || trimmed.starts_with("http://localhost:")
        || trimmed.starts_with("https://localhost:")
}

fn start_local_host(listen: &str) -> Result<ManagedHost, String> {
    let current_executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let target_directory = current_executable
        .parent()
        .ok_or_else(|| "Unable to resolve the stress dashboard target directory.".to_string())?;
    let host_executable = {
        let mut path = target_directory.to_path_buf();
        path.push(executable_name("qsoripper-stress"));
        path
    };

    let mut command = if host_executable.exists() {
        let mut command = Command::new(host_executable);
        command.args(["--listen", listen]);
        command
    } else {
        let workspace_manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| "Unable to resolve Rust workspace root.".to_string())?
            .join("Cargo.toml");
        let mut command = Command::new("cargo");
        command.args([
            "run",
            "--manifest-path",
            workspace_manifest
                .to_str()
                .ok_or_else(|| "Workspace manifest path is not valid UTF-8.".to_string())?,
            "-p",
            "qsoripper-stress",
            "--",
            "--listen",
            listen,
        ]);
        command
    };

    command.stdout(Stdio::null());
    command.stderr(Stdio::null());
    command.stdin(Stdio::null());
    let child = command.spawn().map_err(|error| error.to_string())?;
    Ok(ManagedHost { child })
}

async fn stop_host(host: &mut ManagedHost) {
    let _ = host.child.kill().await;
    let _ = host.child.wait().await;
}

async fn wait_for_endpoint(endpoint: &str) -> Result<(), String> {
    let mut attempts = 0u16;
    while attempts < 240 {
        if can_connect(endpoint).await {
            return Ok(());
        }

        attempts = attempts.saturating_add(1);
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    Err(format!(
        "Timed out waiting for the stress host endpoint '{endpoint}'."
    ))
}

fn endpoint_to_listen_argument(endpoint: &str) -> Result<String, String> {
    let trimmed = endpoint
        .trim()
        .trim_end_matches('/')
        .strip_prefix("http://")
        .or_else(|| {
            endpoint
                .trim()
                .trim_end_matches('/')
                .strip_prefix("https://")
        })
        .unwrap_or(endpoint);
    if trimmed.is_empty() {
        return Err("Stress host endpoint is empty.".to_string());
    }

    Ok(trimmed.to_string())
}

fn format_connection_error(endpoint: &str, details: &str) -> String {
    if is_local_endpoint(endpoint) {
        format!(
            "Unable to reach the stress host at {endpoint}.\n\nStart it with `cargo run -p qsoripper-stress`, or rerun the dashboard after the host is already listening.\n\nDetails: {details}"
        )
    } else {
        format!(
            "Unable to reach the stress host at {endpoint}.\n\nPass a reachable `--endpoint` value or start the stress host there.\n\nDetails: {details}"
        )
    }
}

fn format_status_error(endpoint: &str, error: &tonic::Status) -> String {
    if error.code() == tonic::Code::Unavailable {
        return format_connection_error(endpoint, &error.to_string());
    }

    error.to_string()
}

fn executable_name(base_name: &str) -> OsString {
    if cfg!(windows) {
        OsString::from(format!("{base_name}.exe"))
    } else {
        OsString::from(base_name)
    }
}

fn parse_endpoint<I>(mut args: I) -> String
where
    I: Iterator<Item = String>,
{
    let mut endpoint = "http://127.0.0.1:50061".to_string();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--endpoint" => {
                if let Some(value) = args.next() {
                    endpoint = value;
                }
            }
            "--help" | "-h" => {
                println!(
                    "QsoRipper stress dashboard\n\nUsage:\n  cargo run -p qsoripper-stress-tui -- [--endpoint http://127.0.0.1:50061]"
                );
                std::process::exit(0);
            }
            _ => {}
        }
    }

    endpoint
}

fn draw(frame: &mut Frame, app: &mut App) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(6),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, app, layout[0]);
    draw_summary(frame, app, layout[1]);
    draw_main(frame, app, layout[2]);
    draw_footer(frame, app, layout[3]);

    if let Some(message) = &app.error_message {
        draw_error_modal(frame, message);
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let state = StressRunState::try_from(app.snapshot.state).unwrap_or(StressRunState::Unspecified);
    let profile_name = app
        .profiles
        .get(app.selected_profile)
        .map_or("long-haul", |profile| profile.name.as_str());
    let line = Line::from(vec![
        Span::styled(
            "QsoRipper Stress Dashboard",
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("State: {}", format_run_state(state)),
            state_style(state),
        ),
        Span::raw("  "),
        Span::raw(format!("Profile: {profile_name}")),
        Span::raw("  "),
        Span::raw(format!("Host: {}", app.endpoint)),
    ]);
    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn draw_summary(frame: &mut Frame, app: &App, area: Rect) {
    let process_summary = app
        .snapshot
        .processes
        .iter()
        .map(|process| {
            format!(
                "{} {:>5.1}% CPU {:>8} MiB",
                process.process_name,
                process.cpu_usage_percent,
                process.working_set_bytes / 1024 / 1024
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let content = vec![
        Line::from(format!(
            "Ops {:>8}   {:>7.1}/s   Errors {:>6}   Internal {:>6}",
            app.snapshot.total_operations,
            app.snapshot.operations_per_second,
            app.snapshot.error_count,
            app.snapshot.internal_error_count
        )),
        Line::from(format!("Vectors {}", app.snapshot.vector_statuses.len())),
        Line::from(if process_summary.is_empty() {
            "Processes: awaiting samples".to_string()
        } else {
            process_summary
        }),
    ];
    frame.render_widget(
        Paragraph::new(content).block(Block::default().borders(Borders::ALL).title("Summary")),
        area,
    );
}

fn draw_main(frame: &mut Frame, app: &mut App, area: Rect) {
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(area);
    let right_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
        .split(main_layout[1]);

    draw_vector_table(frame, app, main_layout[0]);
    draw_events(frame, app, right_layout[0]);
    draw_details(frame, app, right_layout[1]);
}

fn draw_vector_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let rows = app.snapshot.vector_statuses.iter().map(|vector| {
        Row::new(vec![
            Cell::from(sanitize_inline_text(vector.display_name.as_str())),
            Cell::from(format_vector_state(vector.state)),
            Cell::from(vector.total_operations.to_string()),
            Cell::from(format!("{:.1}", vector.operations_per_second)),
            Cell::from(vector.error_count.to_string()),
            Cell::from(sanitize_optional_text(vector.last_sample_input.as_ref())),
        ])
        .height(1)
    });
    let table = Table::new(
        rows,
        [
            Constraint::Length(26),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(8),
            Constraint::Min(10),
        ],
    )
    .header(
        Row::new(vec![
            "Vector",
            "State",
            "Ops",
            "Ops/s",
            "Errors",
            "Last sample",
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(if app.focus == Focus::Vectors {
                "Vectors (focused)"
            } else {
                "Vectors"
            }),
    )
    .row_highlight_style(Style::default().bg(Color::Blue));
    frame.render_stateful_widget(table, area, &mut app.vector_table_state);
}

fn draw_events(frame: &mut Frame, app: &mut App, area: Rect) {
    let items = app.snapshot.recent_events.iter().map(|event| {
        let level = StressLogLevel::try_from(event.level).unwrap_or(StressLogLevel::Unspecified);
        let prefix = match level {
            StressLogLevel::Info => "[INFO]",
            StressLogLevel::Warning => "[WARN]",
            StressLogLevel::Error => "[ERR ]",
            StressLogLevel::Unspecified => "[....]",
        };
        ListItem::new(format!(
            "{prefix} {}",
            sanitize_inline_text(event.message.as_str())
        ))
    });
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(if app.focus == Focus::Events {
                    "Events (focused)"
                } else {
                    "Events"
                }),
        )
        .highlight_style(Style::default().bg(Color::Blue));
    frame.render_stateful_widget(list, area, &mut app.event_list_state);
}

fn draw_details(frame: &mut Frame, app: &App, area: Rect) {
    let text = if app.focus == Focus::Vectors {
        app.snapshot
            .vector_statuses
            .get(app.selected_vector())
            .map_or_else(
                || vec![Line::from("No vector selected.")],
                |vector| {
                    vec![
                        Line::from(format!(
                            "Vector: {}",
                            sanitize_inline_text(vector.display_name.as_str())
                        )),
                        Line::from(format!(
                            "State: {}   Ops: {}   Ops/s: {:.1}",
                            format_vector_state(vector.state),
                            vector.total_operations,
                            vector.operations_per_second
                        )),
                        Line::from(format!(
                            "Errors: {}   Internal: {}",
                            vector.error_count, vector.internal_error_count
                        )),
                        Line::from(format!(
                            "Last sample: {}",
                            sanitize_optional_text(vector.last_sample_input.as_ref())
                        )),
                        Line::from(format!(
                            "Last error: {}",
                            sanitize_optional_text(vector.last_error_message.as_ref())
                        )),
                    ]
                },
            )
    } else {
        app.snapshot
            .recent_events
            .get(app.selected_event())
            .map_or_else(
                || vec![Line::from("No event selected.")],
                |event| {
                    vec![
                        Line::from("Event detail"),
                        Line::from(sanitize_inline_text(event.message.as_str())),
                        Line::from(format!(
                            "Vector: {}",
                            sanitize_optional_text(event.vector_id.as_ref())
                        )),
                    ]
                },
            )
    };
    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL).title("Details")),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let footer = Paragraph::new(Line::from(vec![
        Span::styled("[s] Start", Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled("[x] Stop", Style::default().fg(Color::Red)),
        Span::raw("  "),
        Span::styled("[r] Restart", Style::default().fg(Color::Yellow)),
        Span::raw("  "),
        Span::styled("[p] Next profile", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("[tab] Focus", Style::default().fg(Color::Magenta)),
        Span::raw("  "),
        Span::styled("[q] Quit", Style::default().fg(Color::Gray)),
        Span::raw(format!("  Status: {}", app.snapshot.status_message)),
    ]))
    .block(Block::default().borders(Borders::ALL));
    frame.render_widget(footer, area);
}

fn draw_error_modal(frame: &mut Frame, message: &str) {
    let area = centered_rect(70, 20, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(message)
            .block(Block::default().borders(Borders::ALL).title("Error"))
            .style(Style::default().fg(Color::Red)),
        area,
    );
}

fn sanitize_optional_text(value: Option<&String>) -> String {
    value.map_or_else(String::new, |text| sanitize_inline_text(text.as_str()))
}

fn sanitize_inline_text(value: &str) -> String {
    const MAX_CHARS: usize = 96;

    let sanitized: String = value.chars().fold(String::new(), |mut output, character| {
        output.push_str(sanitize_character(character).as_str());
        output
    });
    if sanitized.chars().count() <= MAX_CHARS {
        return sanitized;
    }

    let mut truncated = sanitized
        .chars()
        .take(MAX_CHARS.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

fn sanitize_character(character: char) -> String {
    match character {
        '\n' => String::from("\\n"),
        '\r' => String::from("\\r"),
        '\t' => String::from("\\t"),
        '\0' => String::from("\\0"),
        '\u{7f}'..='\u{9f}'
        | '\u{200e}'
        | '\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}'
        | '\u{feff}' => String::from("?"),
        character if character.is_control() => String::from("?"),
        _ => character.to_string(),
    }
}

fn centered_rect(width_percent: u16, height_percent: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - height_percent) / 2),
            Constraint::Percentage(height_percent),
            Constraint::Percentage((100 - height_percent) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - width_percent) / 2),
            Constraint::Percentage(width_percent),
            Constraint::Percentage((100 - width_percent) / 2),
        ])
        .split(vertical[1])[1]
}

fn state_style(state: StressRunState) -> Style {
    match state {
        StressRunState::Running => Style::default().fg(Color::Green),
        StressRunState::Starting | StressRunState::Stopping => Style::default().fg(Color::Yellow),
        StressRunState::Failed => Style::default().fg(Color::Red),
        StressRunState::Completed | StressRunState::Stopped => Style::default().fg(Color::Cyan),
        _ => Style::default().fg(Color::Gray),
    }
}

fn format_run_state(state: StressRunState) -> &'static str {
    match state {
        StressRunState::Idle => "Idle",
        StressRunState::Starting => "Starting",
        StressRunState::Running => "Running",
        StressRunState::Stopping => "Stopping",
        StressRunState::Completed => "Completed",
        StressRunState::Failed => "Failed",
        StressRunState::Stopped => "Stopped",
        StressRunState::Unspecified => "Unknown",
    }
}

fn format_vector_state(state: i32) -> &'static str {
    match StressVectorState::try_from(state).unwrap_or(StressVectorState::Unspecified) {
        StressVectorState::Idle => "Idle",
        StressVectorState::Running => "Running",
        StressVectorState::Completed => "Done",
        StressVectorState::Failed => "Failed",
        StressVectorState::Unspecified => "Unknown",
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Vectors,
    Events,
}

enum AppEvent {
    Snapshot(Box<StressRunSnapshot>),
    StreamError(String),
}

struct ManagedHost {
    child: Child,
}

struct App {
    endpoint: String,
    profiles: Vec<StressProfile>,
    selected_profile: usize,
    snapshot: StressRunSnapshot,
    focus: Focus,
    vector_table_state: TableState,
    event_list_state: ListState,
    error_message: Option<String>,
}

impl App {
    fn new(endpoint: String, profiles: Vec<StressProfile>, snapshot: StressRunSnapshot) -> Self {
        let mut vector_table_state = TableState::default();
        vector_table_state.select(Some(0));
        let mut event_list_state = ListState::default();
        event_list_state.select(Some(0));
        Self {
            endpoint,
            profiles,
            selected_profile: 0,
            snapshot,
            focus: Focus::Vectors,
            vector_table_state,
            event_list_state,
            error_message: None,
        }
    }

    fn toggle_focus(&mut self) {
        self.focus = if self.focus == Focus::Vectors {
            Focus::Events
        } else {
            Focus::Vectors
        };
    }

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Vectors => {
                let len = self.snapshot.vector_statuses.len();
                self.move_table_selection(delta, len);
            }
            Focus::Events => {
                let len = self.snapshot.recent_events.len();
                self.move_list_selection(delta, len);
            }
        }
    }

    fn move_table_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.vector_table_state.select(None);
            return;
        }
        let current = self.vector_table_state.selected().unwrap_or(0);
        let next = current
            .saturating_add_signed(delta)
            .clamp(0, len.saturating_sub(1));
        self.vector_table_state.select(Some(next));
    }

    fn move_list_selection(&mut self, delta: isize, len: usize) {
        if len == 0 {
            self.event_list_state.select(None);
            return;
        }
        let current = self.event_list_state.selected().unwrap_or(0);
        let next = current
            .saturating_add_signed(delta)
            .clamp(0, len.saturating_sub(1));
        self.event_list_state.select(Some(next));
    }

    fn advance_profile(&mut self) {
        if !self.profiles.is_empty() {
            self.selected_profile = (self.selected_profile + 1) % self.profiles.len();
        }
    }

    fn clamp_selection(&mut self) {
        self.move_table_selection(0, self.snapshot.vector_statuses.len());
        self.move_list_selection(0, self.snapshot.recent_events.len());
    }

    fn selected_vector(&self) -> usize {
        self.vector_table_state.selected().unwrap_or(0)
    }

    fn selected_event(&self) -> usize {
        self.event_list_state.selected().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::{endpoint_to_listen_argument, is_local_endpoint, sanitize_inline_text};

    #[expect(
        clippy::panic,
        reason = "This unit test intentionally panics when the listen argument helper regresses."
    )]
    #[test]
    fn endpoint_to_listen_argument_removes_scheme_and_trailing_slash() {
        let value = endpoint_to_listen_argument("http://127.0.0.1:50061/")
            .unwrap_or_else(|error| panic!("listen argument should parse: {error}"));
        assert_eq!("127.0.0.1:50061", value);
    }

    #[test]
    fn is_local_endpoint_accepts_loopback_and_rejects_remote_hosts() {
        assert!(is_local_endpoint("http://127.0.0.1:50061"));
        assert!(is_local_endpoint("https://localhost:50061"));
        assert!(!is_local_endpoint("http://192.168.1.50:50061"));
    }

    #[test]
    fn sanitize_inline_text_escapes_controls_but_keeps_normal_punctuation() {
        let sanitized = sanitize_inline_text(
            "Profile 'long-haul' selected.\nline2\t\0abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz",
        );
        assert!(!sanitized.contains('\n'));
        assert!(!sanitized.contains('\t'));
        assert!(sanitized.contains("\\n"));
        assert!(sanitized.contains("\\t"));
        assert!(sanitized.contains("'long-haul'"));
        assert!(sanitized.ends_with("..."));
    }
}
