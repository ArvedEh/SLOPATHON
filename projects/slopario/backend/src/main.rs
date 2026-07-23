mod food;
mod game;
mod player;
mod session;
mod ws {
    pub mod controller;
    pub mod display;
}

use axum::{
    Json, Router,
    extract::{
        Path, Query, State,
        ws::{WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

use crate::game::{GameState, start_game_loop};
use crate::session::{SessionMap, create_session_map, generate_session_id};
use crate::ws::controller::handle_controller;
use crate::ws::display::handle_display;

const PORT: u16 = 6969;

// Embed the frontend HTML files into the binary at compile time
const HOST_HTML: &str = include_str!("../host-frontend/index.html");
const CLIENT_HTML: &str = include_str!("../client-frontend/index.html");

#[derive(Debug, Deserialize)]
pub struct CreateSessionQuery {
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Serialize)]
struct CreateSessionResponse {
    session_id: String,
    map_width: u32,
    map_height: u32,
}

#[derive(Clone)]
struct AppState {
    sessions: SessionMap,
}

/// Serviert das Host-Frontend (Display/Lobby) unter /host
async fn serve_host() -> Html<&'static str> {
    Html(HOST_HTML)
}

/// Serviert das Client-Frontend (Controller/Joystick) unter /client
async fn serve_client() -> Html<&'static str> {
    Html(CLIENT_HTML)
}

/// Erstellt eine neue Session und gibt die ID zurück.
/// Query-Parameter: ?width=1920&height=1080 (optional, Default: 1000x800)
/// Der Game-Loop startet noch nicht – erst wenn das erste Display sich verbindet.
async fn create_session(
    State(state): State<AppState>,
    Query(query): Query<CreateSessionQuery>,
) -> Json<CreateSessionResponse> {
    let map_width = query.width.unwrap_or(1000);
    let map_height = query.height.unwrap_or(800);

    let session_id = generate_session_id();
    let session = crate::session::Session::new(session_id.clone(), map_width, map_height);
    let session = std::sync::Arc::new(tokio::sync::Mutex::new(session));

    // Speichere die Session (ohne Game-Loop)
    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(session_id.clone(), session);
    }

    tracing::info!(
        "Created new session: {} ({}x{})",
        session_id,
        map_width,
        map_height
    );

    Json(CreateSessionResponse {
        session_id: session_id.clone(),
        map_width,
        map_height,
    })
}

/// Controller-WebSocket: /ws/controller/:session_id
async fn ws_controller_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_controller_with_session(socket, session_id, state))
}

async fn handle_controller_with_session(socket: WebSocket, session_id: String, state: AppState) {
    let session = {
        let sessions = state.sessions.lock().await;
        sessions.get(&session_id).cloned()
    };

    match session {
        Some(session) => handle_controller(socket, session).await,
        None => {
            tracing::warn!("Session not found: {}", session_id);
        }
    }
}

/// Display-WebSocket: /ws/view/:session_id
async fn ws_display_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_display_with_session(socket, session_id, state))
}

async fn handle_display_with_session(socket: WebSocket, session_id: String, state: AppState) {
    let session = {
        let sessions = state.sessions.lock().await;
        sessions.get(&session_id).cloned()
    };

    match session {
        Some(session_arc) => {
            // Map-Größe auslesen (wird beim Session-Erstellen festgelegt)
            let (map_width, map_height) = {
                let session = session_arc.lock().await;
                (session.map_width, session.map_height)
            };

            // Display verbindet sich: Erstelle broadcast-Channel
            let rx: broadcast::Receiver<GameState> = {
                let mut session = session_arc.lock().await;
                session.connect_display()
            };

            // Starte den Game-Loop nur beim ersten Display
            {
                let mut session = session_arc.lock().await;
                if !session.game_loop_started {
                    session.game_loop_started = true;
                    drop(session);
                    start_game_loop(session_arc.clone());
                }
            }

            // Behandle Display-Verbindung mit dem Receiver + Map-Größe + Session
            handle_display(socket, map_width, map_height, rx, session_arc).await;
        }
        None => {
            tracing::warn!("Session not found: {}", session_id);
        }
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let state = AppState {
        sessions: create_session_map(),
    };

    let app = Router::new()
        .route("/api/session", get(create_session))
        .route("/ws/controller/{session_id}", get(ws_controller_handler))
        .route("/ws/view/{session_id}", get(ws_display_handler))
        .route("/host", get(serve_host))
        .route("/client", get(serve_client))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", PORT);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect(&format!("Failed to bind to port {}", PORT));

    tracing::info!("Server running on http://{}", addr);
    tracing::info!("Endpoints:");
    tracing::info!("  GET /host                                  - Host/Lobby frontend");
    tracing::info!("  GET /client                                - Client/Controller frontend");
    tracing::info!("  GET /api/session[?width=1920&height=1080] - Create session");
    tracing::info!("  GET /ws/controller/{{session_id}}          - Controller WebSocket");
    tracing::info!("  GET /ws/view/{{session_id}}                - Display WebSocket");

    axum::serve(listener, app).await.expect("Server failed");
}
