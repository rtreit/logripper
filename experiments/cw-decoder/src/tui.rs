//! Live TUI: scrolling waveform + decoded text + WPM gauge.

use std::io;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use parking_lot::Mutex;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Sparkline, Wrap};
use ratatui::Terminal;

use crate::audio::LiveCapture;
use crate::decoder::decode_window;
use crate::ditdah_streaming::PrefixStabilizer;
use crate::log_capture::DitdahLogCapture;

const WPM_HISTORY_CAP: usize = 120;

pub struct AppState {
    pub decoded: String,
    pub wpm: Option<f32>,
    pub wpm_smoothed: Option<f32>,
    pub wpm_history: std::collections::VecDeque<f32>,
    pub pitch: Option<f32>,
    pub last_decode: Option<Instant>,
    pub decode_count: u64,
    pub status: String,
}

fn median(values: &[f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    let mut v: Vec<f32> = values.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

pub fn run(capture: LiveCapture, log_capture: DitdahLogCapture) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let state = Arc::new(Mutex::new(AppState {
        decoded: String::new(),
        wpm: None,
        wpm_smoothed: None,
        wpm_history: std::collections::VecDeque::with_capacity(WPM_HISTORY_CAP),
        pitch: None,
        last_decode: None,
        decode_count: 0,
        status: format!("Capturing from: {}", capture.device_name),
    }));

    // Decoder worker thread: every ~1.5s, snapshot the ring buffer and run ditdah.
    let buffer = Arc::clone(&capture.buffer);
    let sample_rate = capture.sample_rate;
    let state_thread = Arc::clone(&state);
    let log_thread = log_capture.clone();
    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_thread = Arc::clone(&stop);
    let worker = std::thread::spawn(move || {
        let min_samples = (sample_rate as f32 * 4.0) as usize; // need ~4s for ditdah
        let mut stabilizer = PrefixStabilizer::new(2);
        while !stop_thread.load(std::sync::atomic::Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(1500));
            let snapshot = {
                let lock = buffer.lock();
                if lock.len() < min_samples {
                    None
                } else {
                    Some(lock.snapshot())
                }
            };
            let Some(samples) = snapshot else {
                let mut s = state_thread.lock();
                s.status = "Buffering audio...".to_string();
                continue;
            };
            match decode_window(&samples, sample_rate, &log_thread) {
                Ok(out) => {
                    let mut s = state_thread.lock();
                    if !out.text.trim().is_empty() {
                        stabilizer.push_snapshot(&out.text);
                        s.decoded = stabilizer.transcript().to_string();
                        // Cap to last ~600 chars
                        if s.decoded.len() > 600 {
                            let start = s.decoded.len() - 600;
                            s.decoded = s.decoded[start..].to_string();
                        }
                    }
                    s.wpm = out.stats.wpm;
                    if let Some(w) = out.stats.wpm {
                        if s.wpm_history.len() == WPM_HISTORY_CAP {
                            s.wpm_history.pop_front();
                        }
                        s.wpm_history.push_back(w);
                        // Median of last up-to-7 samples for stability.
                        let take = s.wpm_history.len().min(7);
                        let recent: Vec<f32> =
                            s.wpm_history.iter().rev().take(take).copied().collect();
                        s.wpm_smoothed = median(&recent);
                    }
                    s.pitch = out.stats.pitch_hz;
                    s.last_decode = Some(Instant::now());
                    s.decode_count += 1;
                    s.status = format!(
                        "Live: {:.1}s window, decode #{}",
                        samples.len() as f32 / sample_rate as f32,
                        s.decode_count
                    );
                }
                Err(e) => {
                    let mut s = state_thread.lock();
                    s.status = format!("decode error: {e}");
                }
            }
        }
    });

    // UI loop
    let mut last_tick = Instant::now();
    let tick_rate = Duration::from_millis(80);
    loop {
        terminal.draw(|f| draw(f, &capture, &state))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => break,
                        KeyCode::Char('c') => {
                            state.lock().decoded.clear();
                        }
                        _ => {}
                    }
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let _ = worker.join();

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn draw(f: &mut ratatui::Frame, capture: &LiveCapture, state: &Arc<Mutex<AppState>>) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header / status
            Constraint::Length(8), // waveform
            Constraint::Length(3), // wpm gauge
            Constraint::Length(6), // wpm history
            Constraint::Min(5),    // decoded text
            Constraint::Length(2), // help
        ])
        .split(f.area());

    let s = state.lock();
    draw_header(f, chunks[0], &s);
    draw_waveform(f, chunks[1], capture);
    draw_wpm(f, chunks[2], &s);
    draw_wpm_history(f, chunks[3], &s);
    draw_text(f, chunks[4], &s);
    draw_help(f, chunks[5]);
}

fn draw_header(f: &mut ratatui::Frame, area: Rect, s: &AppState) {
    let pitch = s
        .pitch
        .map(|p| format!("{p:.0} Hz"))
        .unwrap_or_else(|| "—".into());
    let raw = s
        .wpm
        .map(|w| format!("{w:.0}"))
        .unwrap_or_else(|| "—".into());
    let smooth = s
        .wpm_smoothed
        .map(|w| format!("{w:.0}"))
        .unwrap_or_else(|| "—".into());
    let line = Line::from(vec![
        Span::styled(
            "CW Decoder PoC ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | pitch: "),
        Span::styled(pitch, Style::default().fg(Color::Yellow)),
        Span::raw("  WPM (smoothed): "),
        Span::styled(
            smooth,
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  raw: "),
        Span::styled(raw, Style::default().fg(Color::DarkGray)),
        Span::raw("  | "),
        Span::raw(s.status.clone()),
    ]);
    let p = Paragraph::new(line).block(Block::default().borders(Borders::ALL).title("status"));
    f.render_widget(p, area);
}

fn draw_waveform(f: &mut ratatui::Frame, area: Rect, capture: &LiveCapture) {
    // Take last ~half-second and downsample to fit width.
    let samples = capture.buffer.lock().snapshot();
    let take = (capture.sample_rate as usize / 2).min(samples.len());
    if take == 0 {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("waveform (no audio yet)");
        f.render_widget(block, area);
        return;
    }
    let recent = &samples[samples.len() - take..];
    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_w = inner_w.max(1);
    let bucket = (recent.len() / inner_w).max(1);
    let mut bars: Vec<u64> = Vec::with_capacity(inner_w);
    for chunk in recent.chunks(bucket).take(inner_w) {
        // RMS, scaled for visibility
        let sumsq: f32 = chunk.iter().map(|x| x * x).sum();
        let rms = (sumsq / chunk.len() as f32).sqrt();
        let v = (rms * 4000.0) as u64; // amplification factor
        bars.push(v.min(1000));
    }
    let max = *bars.iter().max().unwrap_or(&1).max(&1);
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("waveform (RMS, ~0.5 s)"),
        )
        .data(&bars)
        .max(max)
        .style(Style::default().fg(Color::Cyan))
        .bar_set(ratatui::symbols::bar::NINE_LEVELS);
    f.render_widget(sparkline, area);
}

fn draw_wpm(f: &mut ratatui::Frame, area: Rect, s: &AppState) {
    let wpm = s.wpm_smoothed.unwrap_or(0.0).clamp(0.0, 50.0);
    let ratio = (wpm / 50.0) as f64;
    let label = if let Some(w) = s.wpm_smoothed {
        format!("{w:.0} WPM (smoothed)")
    } else {
        "— WPM".to_string()
    };
    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("speed (5–40 WPM range)"),
        )
        .gauge_style(Style::default().fg(Color::Green))
        .ratio(ratio)
        .label(label);
    f.render_widget(gauge, area);
}

fn draw_wpm_history(f: &mut ratatui::Frame, area: Rect, s: &AppState) {
    if s.wpm_history.is_empty() {
        let block = Block::default()
            .borders(Borders::ALL)
            .title("WPM history (no decodes yet)");
        f.render_widget(block, area);
        return;
    }
    let inner_w = area.width.saturating_sub(2) as usize;
    let take = s.wpm_history.len().min(inner_w.max(1));
    let data: Vec<u64> = s
        .wpm_history
        .iter()
        .rev()
        .take(take)
        .rev()
        .map(|w| w.round() as u64)
        .collect();
    let max = *data.iter().max().unwrap_or(&1).max(&1);
    let sparkline = Sparkline::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("WPM history (raw, newest right)"),
        )
        .data(&data)
        .max(max)
        .style(Style::default().fg(Color::Magenta));
    f.render_widget(sparkline, area);
}

fn draw_text(f: &mut ratatui::Frame, area: Rect, s: &AppState) {
    let text = if s.decoded.is_empty() {
        "(waiting for first decode…)".to_string()
    } else {
        s.decoded.clone()
    };
    let p = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .block(Block::default().borders(Borders::ALL).title("decoded"));
    f.render_widget(p, area);
}

fn draw_help(f: &mut ratatui::Frame, area: Rect) {
    let line = Line::from(vec![
        Span::styled("q / Esc", Style::default().fg(Color::Yellow)),
        Span::raw(" quit  "),
        Span::styled("c", Style::default().fg(Color::Yellow)),
        Span::raw(" clear decoded text"),
    ]);
    f.render_widget(Paragraph::new(line), area);
}
