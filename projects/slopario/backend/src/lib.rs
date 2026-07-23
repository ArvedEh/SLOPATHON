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
    routing::{any, get},
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::{info, trace, warn, Span};

use crate::game::start_game_loop;
use crate::session::{SessionMap, create_session_map, generate_session_id};
use crate::ws::controller::handle_controller;
use crate::ws::display::handle_display;

// Re-export for tests
pub use crate::food::Food;
pub use crate::game::{GameState, InitState, PlayerInfo, FoodInfo, update};
pub use crate::player::{Player, Direction};
pub use crate::session::{Session, SessionState};

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
pub struct AppState {
    pub sessions: SessionMap,
}

/// Serviert das Host-Frontend (Display/Lobby) unter /host
async fn serve_host() -> Html<&'static str> {
    trace!("Serving /host frontend");
    Html(HOST_HTML)
}

/// Serviert das Client-Frontend (Controller/Joystick) unter /client
async fn serve_client() -> Html<&'static str> {
    trace!("Serving /client frontend");
    Html(CLIENT_HTML)
}

/// Erstellt eine neue Session und gibt die ID zurück.
#[tracing::instrument(skip(state), fields(session_id))]
pub async fn create_session_handler(
    State(state): State<AppState>,
    Query(query): Query<CreateSessionQuery>,
) -> Json<CreateSessionResponse> {
    let map_width = query.width.unwrap_or(1000);
    let map_height = query.height.unwrap_or(800);

    trace!(map_width, map_height, "Creating new session");

    let session_id = generate_session_id();
    let session = Session::new(session_id.clone(), map_width, map_height);
    let session = std::sync::Arc::new(tokio::sync::Mutex::new(session));

    {
        let mut sessions = state.sessions.lock().await;
        sessions.insert(session_id.clone(), session);
    }

    Span::current().record("session_id", &session_id);

    info!(
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

/// Controller-WebSocket: /ws/controller/:session_id (any HTTP method)
#[tracing::instrument(skip(ws, state), fields(session_id))]
async fn ws_controller_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    trace!("WebSocket upgrade request for controller");
    Span::current().record("session_id", &session_id);
    ws.on_upgrade(move |socket| handle_controller_with_session(socket, session_id, state))
}

#[tracing::instrument(skip(socket, state), fields(session_id))]
async fn handle_controller_with_session(socket: WebSocket, session_id: String, state: AppState) {
    Span::current().record("session_id", &session_id);
    trace!("Looking up session for controller");
    let session = {
        let sessions = state.sessions.lock().await;
        sessions.get(&session_id).cloned()
    };

    match session {
        Some(session) => {
            trace!("Session found, starting controller handler");
            handle_controller(socket, session).await;
        }
        None => {
            warn!("Session not found: {}", session_id);
        }
    }
}

/// Display-WebSocket: /ws/view/:session_id (any HTTP method)
#[tracing::instrument(skip(ws, state), fields(session_id))]
async fn ws_display_handler(
    ws: WebSocketUpgrade,
    Path(session_id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    trace!("WebSocket upgrade request for display");
    Span::current().record("session_id", &session_id);
    ws.on_upgrade(move |socket| handle_display_with_session(socket, session_id, state))
}

#[tracing::instrument(skip(socket, state), fields(session_id))]
async fn handle_display_with_session(socket: WebSocket, session_id: String, state: AppState) {
    Span::current().record("session_id", &session_id);
    trace!("Looking up session for display");
    let session = {
        let sessions = state.sessions.lock().await;
        sessions.get(&session_id).cloned()
    };

    match session {
        Some(session_arc) => {
            trace!("Session found, setting up display connection");
            let (map_width, map_height) = {
                let session = session_arc.lock().await;
                (session.map_width, session.map_height)
            };
            trace!(map_width, map_height, "Display map size");

            let rx: broadcast::Receiver<GameState> = {
                let mut session = session_arc.lock().await;
                session.connect_display()
            };

            {
                let mut session = session_arc.lock().await;
                if !session.game_loop_started {
                    session.game_loop_started = true;
                    trace!("Starting game loop for session");
                    drop(session);
                    start_game_loop(session_arc.clone());
                }
            }

            handle_display(socket, map_width, map_height, rx, session_arc).await;
        }
        None => {
            warn!("Session not found: {}", session_id);
        }
    }
}

/// Build the router and app state with CORS enabled.
pub fn build_app() -> (Router, AppState) {
    let state = AppState {
        sessions: create_session_map(),
    };

    let app = Router::new()
        .route("/api/session", get(create_session_handler))
        // WebSocket routes accept any HTTP method for broad browser compatibility
        .route("/ws/controller/{session_id}", any(ws_controller_handler))
        .route("/ws/view/{session_id}", any(ws_display_handler))
        .route("/host", get(serve_host))
        .route("/client", get(serve_client))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    (app, state)
}

/// Start the server on the given port and return the address it's listening on.
pub async fn run_on_port(port: u16) -> String {
    let (app, _state) = build_app();
    let addr = format!("127.0.0.1:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect(&format!("Failed to bind to port {}", port));

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("Server failed");
    });

    format!("127.0.0.1:{}", port)
}

pub const PORT: u16 = 6969;

/// Run the default server (used by main.rs)
pub async fn run_default_server() {
    let (app, _state) = build_app();

    let addr = format!("0.0.0.0:{}", PORT);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect(&format!("Failed to bind to port {}", PORT));

    info!("Server running on http://{}", addr);
    info!("Endpoints:");
    info!("  GET /host                                  - Host/Lobby frontend");
    info!("  GET /client                                - Client/Controller frontend");
    info!("  GET /api/session[?width=1920&height=1080] - Create session");
    info!("  GET /ws/controller/{{session_id}}          - Controller WebSocket");
    info!("  GET /ws/view/{{session_id}}                - Display WebSocket");

    axum::serve(listener, app).await.expect("Server failed");
}