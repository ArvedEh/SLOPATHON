use axum::extract::ws::{Message, WebSocket};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, trace, warn};

use crate::player::Direction;
use crate::session::Session;

#[derive(Debug, Deserialize)]
struct ControllerInput {
    direction: Option<Direction>,
    switch_color: Option<bool>,
}

/// Sends a text line to the controller with newline delimiter.
async fn send_line(sink: &mut futures::stream::SplitSink<WebSocket, Message>, line: &str) -> Result<(), ()> {
    trace!("Sending line to controller: {}", line);
    let msg = Message::Text(format!("{}\n", line).into());
    sink.send(msg).await.map_err(|_| ())
}

/// Behandelt eine Controller-WebSocket-Verbindung.
/// Der Controller sendet nur Richtungsdaten (unidirektional).
/// Das Backend sendet Text-Lines über einen mpsc-Channel:
///   - "lobby"   bei Verbindungsaufbau
///   - "ingame"  wenn das Spiel beginnt
///   - "message: [Text]" für Nachrichten (z.B. "message: rank 3")
#[tracing::instrument(skip(socket, session))]
pub async fn handle_controller(mut socket: WebSocket, session: Arc<Mutex<Session>>) {
    info!("Controller WebSocket connected");

    // Controller bei der Session registrieren
    let player_id = {
        let mut session = session.lock().await;
        session.player_count += 1;
        let player_id = format!("player_{}", session.player_count);
        let color = random_color();
        let x = rand::random::<f64>() * session.map_width as f64;
        let y = rand::random::<f64>() * session.map_height as f64;
        let player = crate::player::Player::new(player_id.clone(), x, y, color.clone());
        trace!(%player_id, %x, %y, %color, "Creating new player for controller");
        session.players.push(Arc::new(Mutex::new(player)));
        player_id
    };

    info!("Controller assigned player: {}", player_id);

    // Erstelle mpsc-Channel für diese Controller-Verbindung
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    {
        let session = session.lock().await;
        // Sende "lobby" sofort
        let _ = tx.send("lobby\n".to_string());
    }
    // Jetzt Sender registrieren (lock freigegeben)
    {
        let mut session = session.lock().await;
        // Alte Sender aufräumen (disconnected)
        let before = session.controller_txs.len();
        session.controller_txs.retain(|s| !s.is_closed());
        let removed = before - session.controller_txs.len();
        if removed > 0 {
            trace!(removed, "Cleaned up disconnected controller senders");
        }
        session.controller_txs.push(tx);
        trace!("Registered controller sender");
    }

    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Task: Sende Nachrichten vom mpsc-Channel an den WebSocket
    let send_task = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if send_line(&mut ws_sender, line.trim_end()).await.is_err() {
                break;
            }
        }
    });

    // Warte auf Richtungs-Updates vom Controller
    while let Some(Ok(msg)) = ws_receiver.next().await {
        match msg {
            Message::Text(text) => {
                info!("Controller {} sent text: {}", player_id, text);
                if let Ok(input) = serde_json::from_str::<ControllerInput>(&text) {
                    let session = session.lock().await;
                    for player in &session.players {
                        let mut player = player.lock().await;
                        if player.id == player_id {
                            if let Some(ref dir) = input.direction {
                                trace!(x = dir.x, y = dir.y, "Updating direction");
                                player.direction = Some(dir.clone());
                            }
                            if let Some(switch) = input.switch_color {
                                if switch {
                                    let new_color = random_color();
                                    trace!(old = %player.color, new = %new_color, "Switching color");
                                    player.color = new_color;
                                }
                            }
                            break;
                        }
                    }
                } else {
                    warn!("Controller {} sent unparseable JSON: {}", player_id, text);
                }
            }
            Message::Binary(data) => {
                info!("Controller {} sent binary data ({} bytes)", player_id, data.len());
            }
            Message::Ping(data) => {
                trace!("Controller {} sent ping ({} bytes)", player_id, data.len());
            }
            Message::Pong(data) => {
                trace!("Controller {} sent pong ({} bytes)", player_id, data.len());
            }
            Message::Close(frame) => {
                info!("Controller {} disconnected: {:?}", player_id, frame);
                break;
            }
        }
    }

    send_task.abort();
    trace!("Controller handler finished");
}

fn random_color() -> String {
    let color = format!(
        "#{:02x}{:02x}{:02x}",
        rand::random::<u8>(),
        rand::random::<u8>(),
        rand::random::<u8>(),
    );
    trace!("Generated random color: {}", color);
    color
}