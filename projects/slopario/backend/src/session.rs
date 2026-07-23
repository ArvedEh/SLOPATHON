use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, broadcast, mpsc};
use uuid::Uuid;

use crate::food::Food;
use crate::player::Player;
use crate::game::GameState;

#[derive(Debug, Clone, PartialEq)]
pub enum SessionState {
    Waiting,
    Running,
}

pub struct Session {
    pub id: String,
    pub state: SessionState,
    pub players: Vec<Arc<Mutex<Player>>>,
    pub food: Vec<Food>,
    pub map_width: u32,
    pub map_height: u32,
    pub display_tx: Option<broadcast::Sender<GameState>>,
    pub player_count: u32,
    pub game_loop_started: bool,
    pub next_food_id: u64,
    /// Senders to send text lines to all connected controllers (lobby/ingame/message).
    pub controller_txs: Vec<mpsc::UnboundedSender<String>>,
}

impl Session {
    pub fn new(id: String, map_width: u32, map_height: u32) -> Self {
        // Start with no players – they are added dynamically when controllers connect
        let players: Vec<Arc<Mutex<Player>>> = Vec::new();

        let food_count = (map_width * map_height / 50000).max(20) as usize;
        let mut food = Vec::with_capacity(food_count);
        for i in 0..food_count {
            let margin = 10.0;
            food.push(Food {
                id: i as u64,
                x: rand::random::<f64>() * (map_width as f64 - 2.0 * margin) + margin,
                y: rand::random::<f64>() * (map_height as f64 - 2.0 * margin) + margin,
                size: 5.0 + rand::random::<f64>() * 5.0,
            });
        }

        Self {
            id,
            state: SessionState::Waiting,
            players,
            food,
            map_width,
            map_height,
            display_tx: None,
            player_count: 0,
            game_loop_started: false,
            next_food_id: food_count as u64,
            controller_txs: Vec::new(),
        }
    }

    /// Verbindet ein Display mit dieser Session.
    /// Erstellt den broadcast-Channel beim ersten Display.
    /// Gibt einen Receiver für Game-State-Updates zurück.
    pub fn connect_display(&mut self) -> broadcast::Receiver<GameState> {
        if self.display_tx.is_none() {
            let (tx, _) = broadcast::channel(32);
            self.display_tx = Some(tx);
        }
        self.display_tx.as_ref().unwrap().subscribe()
    }

    /// Broadcastet eine Textzeile an alle verbundenen Controller.
    pub fn broadcast_to_controllers(&self, line: &str) {
        let msg = format!("{}\n", line);
        for tx in &self.controller_txs {
            let _ = tx.send(msg.clone());
        }
    }

    pub fn to_game_state(&self) -> GameState {
        let players = self
            .players
            .iter()
            .filter_map(|p| {
                let p = p.try_lock().ok()?;
                if p.alive {
                    Some(crate::game::PlayerInfo {
                        id: p.id.clone(),
                        x: p.x,
                        y: p.y,
                        size: p.size,
                        color: p.color.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let food = self
            .food
            .iter()
            .map(|f| crate::game::FoodInfo {
                x: f.x,
                y: f.y,
                size: f.size,
            })
            .collect();

        GameState { players, food }
    }
}

pub type SessionMap = Arc<Mutex<HashMap<String, Arc<Mutex<Session>>>>>;

pub fn create_session_map() -> SessionMap {
    Arc::new(Mutex::new(HashMap::new()))
}

pub fn generate_session_id() -> String {
    Uuid::new_v4().to_string()
}