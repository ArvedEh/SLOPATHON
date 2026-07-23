use axum::{
    extract::ws::{Message, WebSocket, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing_subscriber::EnvFilter;

async fn ws_handler(
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(socket: WebSocket) {
    let (sender, mut receiver) = socket.split();
    let sender = Arc::new(Mutex::new(sender));

    tracing::info!("WebSocket client connected");

    while let Some(Ok(msg)) = receiver.next().await {
        match msg {
            Message::Text(text) => {
                tracing::debug!("Received text: {}", text);

                // Echo the message back
                let mut sender = sender.lock().await;
                if let Err(e) = sender
                    .send(Message::Text(format!("Echo: {}", text)))
                    .await
                {
                    tracing::error!("Failed to send message: {}", e);
                    break;
                }
            }
            Message::Binary(data) => {
                tracing::debug!("Received binary data ({} bytes)", data.len());

                // Echo the binary data back
                let mut sender = sender.lock().await;
                if let Err(e) = sender.send(Message::Binary(data)).await {
                    tracing::error!("Failed to send binary: {}", e);
                    break;
                }
            }
            Message::Ping(data) => {
                let mut sender = sender.lock().await;
                if let Err(e) = sender.send(Message::Pong(data)).await {
                    tracing::error!("Failed to send pong: {}", e);
                    break;
                }
            }
            Message::Pong(_) => {
                // Ignore pongs
            }
            Message::Close(_) => {
                tracing::info!("Client disconnected gracefully");
                break;
            }
        }
    }

    tracing::info!("WebSocket connection closed");
}

async fn health_check() -> &'static str {
    "OK"
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let app = Router::new()
        .route("/", get(health_check))
        .route("/ws", get(ws_handler));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .expect("Failed to bind to port 3000");

    tracing::info!("Server running on http://0.0.0.0:3000");

    axum::serve(listener, app)
        .await
        .expect("Server failed");
}