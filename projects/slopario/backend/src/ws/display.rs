use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;
use tracing::{info, trace, warn};

use crate::game::{GameState, InitState};
use crate::session::{Session, SessionState};

/// Behandelt eine Display-WebSocket-Verbindung.
/// Sendet zuerst die Init-Nachricht mit der Map-Größe,
/// wartet dann auf "start" vom Display, und streamt dann 30fps Game-State-Updates.
#[tracing::instrument(skip(socket, rx, session))]
pub async fn handle_display(
    mut socket: WebSocket,
    map_width: u32,
    map_height: u32,
    mut rx: broadcast::Receiver<GameState>,
    session: Arc<Mutex<Session>>,
) {
    info!("Display WebSocket connected");

    // 1. Init-Nachricht mit Map-Größe senden
    let init = InitState::new(map_width, map_height);
    let init_json = serde_json::to_string(&init).unwrap();
    trace!(%init_json, "Sending InitState to display");
    if socket.send(Message::Text(init_json.into())).await.is_err() {
        warn!("Display disconnected before receiving init");
        return;
    }
    trace!("InitState sent successfully");

    // 2. Auf "start"-Kommando vom Display warten
    trace!("Waiting for 'start' command from display");
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                trace!("Received message from display: {}", text);
                if text.trim() == "start" {
                    info!("Display started the game");
                    let mut session = session.lock().await;
                    session.state = SessionState::Running;
                    session.broadcast_to_controllers("ingame");
                    trace!("Broadcasted 'ingame' to all controllers");
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                warn!("Display disconnected before sending 'start'");
                return;
            }
            _ => {}
        }
    }

    // 3. Game-State-Updates streamen
    trace!("Starting to stream GameState updates");
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(state) => {
                        let json = serde_json::to_string(&state).unwrap();
                        trace!("Sending GameState to display ({} bytes)", json.len());
                        if socket.send(Message::Text(json.into())).await.is_err() {
                            warn!("Display disconnected during streaming");
                            break;
                        }
                    }
                    Err(_) => {
                        trace!("GameState broadcast channel closed, stopping stream");
                        break;
                    }
                }
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        info!("Display disconnected from game stream");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    trace!("Display handler finished");
}