use axum::extract::ws::{Message, WebSocket};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::sync::Mutex;

use crate::game::{GameState, InitState};
use crate::session::{Session, SessionState};

/// Behandelt eine Display-WebSocket-Verbindung.
/// Sendet zuerst die Init-Nachricht mit der Map-Größe,
/// wartet dann auf "start" vom Display, und streamt dann 30fps Game-State-Updates.
pub async fn handle_display(
    mut socket: WebSocket,
    map_width: u32,
    map_height: u32,
    mut rx: broadcast::Receiver<GameState>,
    session: Arc<Mutex<Session>>,
) {
    tracing::info!("Display connected");

    // 1. Init-Nachricht mit Map-Größe senden
    let init = InitState::new(map_width, map_height);
    let init_json = serde_json::to_string(&init).unwrap();
    if socket.send(Message::Text(init_json)).await.is_err() {
        return;
    }

    // 2. Auf "start"-Kommando vom Display warten
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                if text.trim() == "start" {
                    tracing::info!("Display started the game");
                    let mut session = session.lock().await;
                    session.state = SessionState::Running;
                    session.broadcast_to_controllers("ingame");
                    break;
                }
            }
            Some(Ok(Message::Close(_))) | None => {
                tracing::info!("Display disconnected before start");
                return;
            }
            _ => {}
        }
    }

    // 3. Game-State-Updates streamen
    loop {
        tokio::select! {
            result = rx.recv() => {
                match result {
                    Ok(state) => {
                        let json = serde_json::to_string(&state).unwrap();
                        if socket.send(Message::Text(json)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            msg = socket.next() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => {
                        tracing::info!("Display disconnected");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}