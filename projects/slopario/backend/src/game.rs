use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

use crate::food::random_food;
use crate::session::{Session, SessionState};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerInfo {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoodInfo {
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    pub players: Vec<PlayerInfo>,
    pub food: Vec<FoodInfo>,
}

/// Wird beim Display-Connect als erste Nachricht gesendet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitState {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub map_width: u32,
    pub map_height: u32,
}

impl InitState {
    pub fn new(map_width: u32, map_height: u32) -> Self {
        Self {
            msg_type: "init".to_string(),
            map_width,
            map_height,
        }
    }
}

/// Wendet einen Game-Tick an: Bewegung, Fressen, Tod.
/// `dt` ist die vergangene Zeit in Sekunden (sollte ~0.033 sein bei 30fps).
/// Diese Funktion ist NICHT async – sie verwendet nur `try_lock`.
pub fn update(session: &mut Session, dt: f64) {
    if session.state != SessionState::Running {
        return;
    }

    let map_w = session.map_width as f64;
    let map_h = session.map_height as f64;
    let speed = map_w / 10.0; // in 10 Sekunden quer über die Map

    // 1. Bewegung
    for player_arc in &session.players {
        let mut player = match player_arc.try_lock() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if !player.alive {
            continue;
        }

        if let Some(ref dir) = player.direction {
            let len = (dir.x * dir.x + dir.y * dir.y).sqrt();
            if len > 0.0 {
                let size_factor = (20.0 / player.size).sqrt().min(1.0);
                let move_speed = speed * size_factor * dt;
                let nx = player.x + (dir.x / len) * move_speed;
                let ny = player.y + (dir.y / len) * move_speed;

                let radius = player.size / 2.0;
                player.x = nx.clamp(radius, map_w - radius);
                player.y = ny.clamp(radius, map_h - radius);
            }
        }
    }

    // 2. Food einsammeln
    let mut eaten_food: Vec<usize> = Vec::new();
    for (fi, food) in session.food.iter().enumerate() {
        for player_arc in &session.players {
            let player = match player_arc.try_lock() {
                Ok(p) => p,
                Err(_) => continue,
            };
            if !player.alive {
                continue;
            }
            let dx = player.x - food.x;
            let dy = player.y - food.y;
            let dist = (dx * dx + dy * dy).sqrt();
            if dist < player.size / 2.0 {
                eaten_food.push(fi);
                break;
            }
        }
    }

    // Food entfernen und Spieler wachsen lassen
    for &fi in eaten_food.iter().rev() {
        if let Some(food) = session.food.get(fi) {
            let food_size = food.size;
            let food_x = food.x;
            let food_y = food.y;
            for player_arc in &session.players {
                let mut player = match player_arc.try_lock() {
                    Ok(p) => p,
                    Err(_) => continue,
                };
                if !player.alive {
                    continue;
                }
                let dx = player.x - food_x;
                let dy = player.y - food_y;
                let dist = (dx * dx + dy * dy).sqrt();
                if dist < player.size / 2.0 {
                    player.size = (player.size * player.size + food_size * food_size).sqrt();
                    player.score += 1;
                    break;
                }
            }
        }
        session.food.remove(fi);
    }

    // Neues Food spawnen
    for _ in &eaten_food {
        let new_food = random_food(session.next_food_id, session.map_width, session.map_height);
        session.next_food_id += 1;
        session.food.push(new_food);
    }

    // 3. Spieler fressen Spieler
    let player_count = session.players.len();
    let mut dead_players: Vec<String> = Vec::new();

    // Sammle Infos über alle Spieler (um nested try_lock zu vermeiden)
    struct PlayerSnapshot {
        index: usize,
        id: String,
        x: f64,
        y: f64,
        size: f64,
        alive: bool,
        score: u32,
    }

    let snapshots: Vec<PlayerSnapshot> = session
        .players
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let p = p.try_lock().ok()?;
            Some(PlayerSnapshot {
                index: i,
                id: p.id.clone(),
                x: p.x,
                y: p.y,
                size: p.size,
                alive: p.alive,
                score: p.score,
            })
        })
        .collect();

    for i in 0..snapshots.len() {
        for j in (i + 1)..snapshots.len() {
            let pi = &snapshots[i];
            let pj = &snapshots[j];
            if !pi.alive || !pj.alive {
                continue;
            }

            let can_i_eat_j = pi.size > pj.size * 1.15;
            let can_j_eat_i = pj.size > pi.size * 1.15;

            if !can_i_eat_j && !can_j_eat_i {
                continue;
            }

            let dx = pi.x - pj.x;
            let dy = pi.y - pj.y;
            let dist = (dx * dx + dy * dy).sqrt();

            if can_i_eat_j && dist < pi.size / 2.0 {
                // i frisst j
                if let Ok(mut pj_mutex) = session.players[pj.index].try_lock() {
                    pj_mutex.alive = false;
                }
                if let Ok(mut pi_mutex) = session.players[pi.index].try_lock() {
                    pi_mutex.size = (pi_mutex.size * pi_mutex.size + pj.size * pj.size).sqrt();
                    pi_mutex.score += pj.score + 1;
                }
                dead_players.push(pj.id.clone());
            } else if can_j_eat_i && dist < pj.size / 2.0 {
                // j frisst i
                if let Ok(mut pi_mutex) = session.players[pi.index].try_lock() {
                    pi_mutex.alive = false;
                }
                if let Ok(mut pj_mutex) = session.players[pj.index].try_lock() {
                    pj_mutex.size = (pj_mutex.size * pj_mutex.size + pi.size * pi.size).sqrt();
                    pj_mutex.score += pi.score + 1;
                }
                dead_players.push(pi.id.clone());
            }
        }
    }

    // 4. Todes-Nachrichten
    let alive_count = snapshots.iter().filter(|s| s.alive).count();
    let total_players = snapshots.len();

    for _ in &dead_players {
        let rank = total_players - alive_count + 1;
        session.broadcast_to_controllers(&format!("message: rank {}", rank));
    }

    // 5. Spielende prüfen
    let still_alive = snapshots.iter().filter(|s| {
        if s.alive {
            // Check if still alive after this tick
            session.players[s.index]
                .try_lock()
                .map(|p| p.alive)
                .unwrap_or(false)
        } else {
            false
        }
    }).count();

    if still_alive <= 1 && session.state == SessionState::Running {
        if let Some(winner) = session.players.iter().find(|p| {
            p.try_lock().map(|p| p.alive).unwrap_or(false)
        }) {
            if let Ok(winner) = winner.try_lock() {
                session
                    .broadcast_to_controllers(&format!("message: winner {}", winner.id));
            }
        }
        session.state = SessionState::Waiting;
    }
}

/// Startet den Game-Loop für eine Session.
/// Läuft mit 30 Ticks pro Sekunde und broadcasted den Game-State an das Display.
pub fn start_game_loop(session: Arc<Mutex<Session>>) {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(33)); // ~30 fps
        let dt = 1.0 / 30.0;

        loop {
            ticker.tick().await;

            // Update game state
            {
                let mut session = session.lock().await;
                update(&mut session, dt);
            }

            // Broadcast to display
            let game_state = {
                let session = session.lock().await;
                session.to_game_state()
            };

            let session = session.lock().await;
            if let Some(ref tx) = session.display_tx {
                if tx.receiver_count() > 0 {
                    let _ = tx.send(game_state);
                }
            }
        }
    });
}