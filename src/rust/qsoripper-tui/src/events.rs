//! Application event type and background-task spawning.

use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time;

use crate::app::{CallsignInfo, RecentQso, SpaceWeatherInfo};
use crate::grpc;

/// Events produced by background tasks and forwarded to the main event loop.
pub(crate) enum AppEvent {
    /// A key press received from the terminal.
    Key(crossterm::event::KeyEvent),
    /// 1-second clock tick for updating the time display.
    Tick,
    /// Result of a callsign lookup; `None` if not found or lookup failed.
    LookupResult(Option<CallsignInfo>),
    /// Current space weather snapshot; `None` if unavailable.
    SpaceWeather(Option<SpaceWeatherInfo>),
    /// A QSO was successfully logged; value is the assigned local ID.
    QsoLogged(String),
    /// A QSO log attempt failed; value is the human-readable error message.
    QsoLogFailed(String),
    /// An existing QSO was successfully updated; value is the local ID.
    QsoUpdated(String),
    /// A QSO update attempt failed; value is the human-readable error message.
    QsoUpdateFailed(String),
    /// A QSO was successfully deleted; value is the deleted local ID.
    QsoDeleted(String),
    /// A QSO deletion attempt failed; value is the human-readable error message.
    QsoDeleteFailed(String),
    /// Refreshed snapshot of recent QSOs.
    RecentQsos(Vec<RecentQso>),
    /// Background name enrichment result for one QSO in the recent list.
    QsoNameEnriched {
        /// The QSO whose name was resolved.
        local_id: String,
        /// Operator name from the lookup cache.
        name: String,
    },
}

/// Spawn a blocking OS thread that reads crossterm key events and forwards them to `tx`.
pub(crate) fn spawn_key_task(tx: mpsc::UnboundedSender<AppEvent>) {
    std::thread::spawn(move || loop {
        match crossterm::event::poll(Duration::from_millis(100)) {
            Ok(true) => match crossterm::event::read() {
                Ok(crossterm::event::Event::Key(key)) => {
                    // Only handle Press events; crossterm on Windows also emits
                    // Release/Repeat which would double every character.
                    if key.kind != crossterm::event::KeyEventKind::Press {
                        continue;
                    }
                    if tx.send(AppEvent::Key(key)).is_err() {
                        break;
                    }
                }
                Ok(_) => {}
                Err(_) => break,
            },
            Ok(false) => {
                if tx.is_closed() {
                    break;
                }
            }
            Err(_) => break,
        }
    });
}

/// Spawn a tokio task that sends a [`AppEvent::Tick`] every second.
pub(crate) fn spawn_clock_task(tx: mpsc::UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            if tx.send(AppEvent::Tick).is_err() {
                break;
            }
        }
    });
}

/// Spawn the callsign lookup debounce task.
///
/// Watches for callsign changes on `lookup_rx`, debounces by 250 ms, then fires a
/// gRPC lookup and sends [`AppEvent::LookupResult`] to `event_tx`.
pub(crate) fn spawn_lookup_task(
    mut lookup_rx: watch::Receiver<String>,
    event_tx: mpsc::UnboundedSender<AppEvent>,
    endpoint: String,
) {
    tokio::spawn(async move {
        loop {
            if lookup_rx.changed().await.is_err() {
                break;
            }
            let callsign = lookup_rx.borrow().clone();
            if callsign.len() < 3 {
                let _ = event_tx.send(AppEvent::LookupResult(None));
                continue;
            }
            time::sleep(Duration::from_millis(250)).await;
            let current = lookup_rx.borrow().clone();
            if current != callsign {
                continue;
            }
            let result = match grpc::create_channel(&endpoint).await {
                Ok(ch) => grpc::lookup_callsign(ch, &callsign).await.ok().flatten(),
                Err(_) => None,
            };
            if event_tx.send(AppEvent::LookupResult(result)).is_err() {
                break;
            }
        }
    });
}
