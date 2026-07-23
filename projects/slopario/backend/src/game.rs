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
                    player.size += food_size * 0.5;
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
    let _player_count = session.players.len();
    let mut dead_players: Vec<String> = Vec::new();

    // Sammle Infos über alle Spieler (um nested try_lock zu vermeiden)
    #[derive(Clone)]
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
                // i eats j
                if let Ok(mut pj_mutex) = session.players[pj.index].try_lock() {
                    pj_mutex.alive = false;
                }
                if let Ok(mut pi_mutex) = session.players[pi.index].try_lock() {
                    pi_mutex.size += pj.size * 0.5;
                    pi_mutex.score += pj.score + 1;
                }
                dead_players.push(pj.id.clone());
            } else if can_j_eat_i && dist < pj.size / 2.0 {
                // j eats i
                if let Ok(mut pi_mutex) = session.players[pi.index].try_lock() {
                    pi_mutex.alive = false;
                }
                if let Ok(mut pj_mutex) = session.players[pj.index].try_lock() {
                    pj_mutex.size += pi.size * 0.5;
                    pj_mutex.score += pi.score + 1;
                }
                dead_players.push(pi.id.clone());
            }
        }
    }

    // 4. Todes-Nachrichten
    // Count alive players AFTER deaths in this tick
    let alive_after_deaths = session.players.iter()
        .filter(|p| p.try_lock().map(|p| p.alive).unwrap_or(false))
        .count();

    for (death_index, _) in dead_players.iter().enumerate() {
        let rank = alive_after_deaths + death_index + 1;
        session.broadcast_to_controllers(&format!("message: rank {}", rank));
    }

    // 5. Spielende prüfen
    let still_alive = snapshots
        .iter()
        .filter(|s| {
            if s.alive {
                session.players[s.index]
                    .try_lock()
                    .map(|p| p.alive)
                    .unwrap_or(false)
            } else {
                false
            }
        })
        .count();

    if still_alive <= 1 && session.state == SessionState::Running {
        if let Some(winner) = session
            .players
            .iter()
            .find(|p| p.try_lock().map(|p| p.alive).unwrap_or(false))
        {
            if let Ok(winner) = winner.try_lock() {
                session.broadcast_to_controllers(&format!("message: winner {}", winner.id));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::food::Food;
    use crate::player::{Direction, Player};
    use std::sync::Arc;
    use tokio::sync::{Mutex, mpsc};

    /// Helper: create a session with specific players and food, state=Running.
    fn make_session(
        players: Vec<Player>,
        food: Vec<Food>,
        map_width: u32,
        map_height: u32,
    ) -> Session {
        // Replace with our controlled data
        let our_players = players
            .into_iter()
            .map(|p| Arc::new(Mutex::new(p)))
            .collect();
        // Disable hungry default food – use provided food
        Session {
            id: "test".into(),
            state: SessionState::Running,
            players: our_players,
            food,
            map_width,
            map_height,
            display_tx: None,
            player_count: 0,
            game_loop_started: false,
            next_food_id: 999,
            controller_txs: Vec::new(),
        }
    }

    /// Helper: run n ticks for deterministic tests.
    fn run_ticks(session: &mut Session, ticks: u32, dt: f64) {
        for _ in 0..ticks {
            update(session, dt);
        }
    }

    // ----------------------------------------------------------------
    //  Movement
    // ----------------------------------------------------------------

    #[test]
    fn test_movement_moves_player_in_direction() {
        let mut player = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        // Give it a direction
        player.direction = Some(Direction { x: 1.0, y: 0.0 });
        let food = vec![];
        let mut session = make_session(vec![player], food, 1000, 800);
        run_ticks(&mut session, 30, 1.0 / 30.0); // ~1 second

        let p = session.players[0].try_lock().unwrap();
        assert!(p.x > 500.0, "Player should have moved right");
        assert!((p.y - 400.0).abs() < 0.1, "Player should not have moved vertically");
    }

    #[test]
    fn test_no_direction_means_no_movement() {
        let player = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        // direction stays None
        let food = vec![];
        let mut session = make_session(vec![player], food, 1000, 800);
        let x_before = session.players[0].try_lock().unwrap().x;
        run_ticks(&mut session, 30, 1.0 / 30.0);
        let x_after = session.players[0].try_lock().unwrap().x;
        assert!((x_after - x_before).abs() < 0.01, "Player without direction should not move");
    }

    #[test]
    fn test_diagonal_movement_normalized() {
        let mut player = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        player.direction = Some(Direction {
            x: 1.0,
            y: 1.0,
        });
        let food = vec![];
        let mut session = make_session(vec![player], food, 1000, 800);
        run_ticks(&mut session, 30, 1.0 / 30.0);
        let p = session.players[0].try_lock().unwrap();
        // Should have moved diagonally (equal x and y offset)
        let dx = p.x - 500.0;
        let dy = p.y - 400.0;
        assert!(dx > 0.0 && dy > 0.0, "Should move diagonally positive");
        assert!((dx - dy).abs() < 0.1, "Diagonal movement should be symmetric");
    }

    #[test]
    fn test_boundary_clamping_left_edge() {
        let mut player = Player::new("p1".into(), 10.0, 400.0, "#ff0000".into());
        player.direction = Some(Direction {
            x: -1.0,
            y: 0.0,
        });
        let food = vec![];
        let mut session = make_session(vec![player], food, 1000, 800);
        // Move left many ticks
        for _ in 0..300 {
            update(&mut session, 1.0 / 30.0);
            let p = session.players[0].try_lock().unwrap();
            // radius = size/2 = 10
            assert!(
                p.x >= 10.0,
                "Player should not cross left edge (x={})",
                p.x
            );
        }
        let p = session.players[0].try_lock().unwrap();
        assert!((p.x - 10.0).abs() < 0.001, "Player should be clamped at left edge");
    }

    #[test]
    fn test_boundary_clamping_right_edge() {
        let mut player = Player::new("p1".into(), 990.0, 400.0, "#ff0000".into());
        player.direction = Some(Direction { x: 1.0, y: 0.0 });
        let food = vec![];
        let mut session = make_session(vec![player], food, 1000, 800);
        run_ticks(&mut session, 300, 1.0 / 30.0);
        let p = session.players[0].try_lock().unwrap();
        assert!(
            p.x <= 990.0,
            "Player should not cross right edge (x={})",
            p.x
        );
    }

    #[test]
    fn test_no_movement_in_waiting_state() {
        let mut player = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        player.direction = Some(Direction { x: 1.0, y: 0.0 });
        let food = vec![];
        let mut session = make_session(vec![player], food, 1000, 800);
        session.state = SessionState::Waiting;
        let x_before = session.players[0].try_lock().unwrap().x;
        run_ticks(&mut session, 30, 1.0 / 30.0);
        let x_after = session.players[0].try_lock().unwrap().x;
        assert!(
            (x_after - x_before).abs() < 0.01,
            "Player should not move in Waiting state"
        );
    }

    // ----------------------------------------------------------------
    //  Food Eating
    // ----------------------------------------------------------------

    #[test]
    fn test_eat_food_player_grows() {
        // Player at center, food directly on top
        let player = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        // size=20, radius=10, so anything within 10 units is eaten
        let food = vec![Food {
            id: 0,
            x: 505.0,
            y: 400.0,
            size: 8.0,
        }];
        let mut session = make_session(vec![player], food, 1000, 800);
        assert_eq!(session.food.len(), 1);

        let size_before = session.players[0].try_lock().unwrap().size;
        run_ticks(&mut session, 1, 1.0 / 30.0);

        let p = session.players[0].try_lock().unwrap();
        assert!(p.size > size_before, "Player should grow after eating food");
        assert_eq!(p.score, 1, "Player score should increase by 1");
        assert_eq!(session.food.len(), 1, "Eaten food should be replaced by new food");
    }

    #[test]
    fn test_food_too_far_not_eaten() {
        let player = Player::new("p1".into(), 100.0, 100.0, "#ff0000".into());
        let food = vec![Food {
            id: 0,
            x: 500.0,
            y: 500.0,
            size: 8.0,
        }];
        let mut session = make_session(vec![player], food, 1000, 800);
        let food_count_before = session.food.len();
        let size_before = session.players[0].try_lock().unwrap().size;
        run_ticks(&mut session, 1, 1.0 / 30.0);
        let size_after = session.players[0].try_lock().unwrap().size;
        assert!(
            (size_after - size_before).abs() < 0.001,
            "Player should not grow when food is far away"
        );
        assert_eq!(
            session.food.len(),
            food_count_before,
            "Food should not be removed"
        );
    }

    // ----------------------------------------------------------------
    //  PvP Eating
    // ----------------------------------------------------------------

    #[test]
    fn test_bigger_eats_smaller() {
        // p1 (size 50) at center, p2 (size 10) on top of p1
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 50.0;
        // p2 starts very close to p1 center – distance 5 < 25 (p1 radius)
        let mut p2 = Player::new("p2".into(), 503.0, 400.0, "#00ff00".into());
        p2.size = 10.0;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        run_ticks(&mut session, 1, 1.0 / 30.0);

        // p2 should be dead
        let p2_locked = session.players[1].try_lock().unwrap();
        assert!(!p2_locked.alive, "Smaller player should be dead after PvP");
    }

    #[test]
    fn test_equal_sizes_dont_eat() {
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 20.0;
        let mut p2 = Player::new("p2".into(), 505.0, 400.0, "#00ff00".into());
        p2.size = 20.0;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        run_ticks(&mut session, 1, 1.0 / 30.0);

        let p1_alive = session.players[0].try_lock().unwrap().alive;
        let p2_alive = session.players[1].try_lock().unwrap().alive;
        assert!(p1_alive && p2_alive, "Equal-sized players should not eat each other");
    }

    #[test]
    fn test_size_ratio_below_threshold_no_eat() {
        // p1 is exactly 1.14x p2 – below the 1.15 threshold
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 22.8; // 20 * 1.14 = 22.8
        let mut p2 = Player::new("p2".into(), 505.0, 400.0, "#00ff00".into());
        p2.size = 20.0;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        run_ticks(&mut session, 1, 1.0 / 30.0);

        let p2_alive = session.players[1].try_lock().unwrap().alive;
        assert!(p2_alive, "1.14x size should not be enough to eat (threshold is 1.15)");
    }

    #[test]
    fn test_big_eats_small_and_grows() {
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 50.0;
        p1.score = 5;
        let mut p2 = Player::new("p2".into(), 503.0, 400.0, "#00ff00".into());
        p2.size = 10.0;
        p2.score = 2;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        run_ticks(&mut session, 1, 1.0 / 30.0);

        let p1_locked = session.players[0].try_lock().unwrap();
        // New size = 50 + 10 * 0.5 = 55.0
        let expected_size = 55.0;
        assert!(
            (p1_locked.size - expected_size).abs() < 0.1,
            "Big player should absorb small player's mass (expected {}, got {})",
            expected_size,
            p1_locked.size
        );
        // Score: 5 (own) + 2 (victim's) + 1 (kill bonus) = 8
        assert_eq!(p1_locked.score, 8, "Player should gain victim's score + 1");
    }

    // ----------------------------------------------------------------
    //  Rank & Winner Messages
    // ----------------------------------------------------------------

    #[test]
    fn test_death_broadcasts_rank_message() {
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 50.0;
        let mut p2 = Player::new("p2".into(), 503.0, 400.0, "#00ff00".into());
        p2.size = 10.0;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        // Add a controller tx so we can capture messages
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        session.controller_txs.push(tx);

        run_ticks(&mut session, 1, 1.0 / 30.0);

        // Should have received a "message: rank 2\n" (2 players, 1 alive → rank 2)
        let msg = rx.try_recv().ok();
        assert!(
            msg.is_some(),
            "Should have received a death message"
        );
        if let Some(text) = msg {
            assert!(text.contains("rank 2"), "Death message should say 'rank 2', got: {}", text);
        }
    }

    #[test]
    fn test_last_player_wins() {
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 50.0;
        let mut p2 = Player::new("p2".into(), 503.0, 400.0, "#00ff00".into());
        p2.size = 10.0;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        session.controller_txs.push(tx);

        // Kill p2
        run_ticks(&mut session, 1, 1.0 / 30.0);

        // Now only p1 alive – run another tick to trigger win detection
        run_ticks(&mut session, 1, 1.0 / 30.0);

        // Should have a winner message
        let mut found_winner = false;
        while let Ok(msg) = rx.try_recv() {
            if msg.contains("winner") {
                found_winner = true;
                assert!(
                    msg.contains("p1"),
                    "Winner should be p1, got: {}",
                    msg
                );
            }
        }
        assert!(found_winner, "Should have received a winner message");
        assert_eq!(
            session.state,
            SessionState::Waiting,
            "Game should return to Waiting state after end"
        );
    }

    // ----------------------------------------------------------------
    //  Size / Speed relationship
    // ----------------------------------------------------------------

    #[test]
    fn test_bigger_player_moves_slower() {
        let mut small = Player::new("small".into(), 100.0, 400.0, "#ff0000".into());
        small.size = 20.0;
        small.direction = Some(Direction { x: 1.0, y: 0.0 });

        let mut big = Player::new("big".into(), 500.0, 400.0, "#00ff00".into());
        big.size = 80.0;
        big.direction = Some(Direction { x: 1.0, y: 0.0 });

        let players = vec![small, big];
        let mut session = make_session(players, vec![], 2000, 800);

        run_ticks(&mut session, 30, 1.0 / 30.0);

        let small_dist = {
            let p = session.players[0].try_lock().unwrap();
            p.x - 100.0
        };
        let big_dist = {
            let p = session.players[1].try_lock().unwrap();
            p.x - 500.0
        };

        assert!(
            small_dist > big_dist,
            "Smaller player (dist={}) should move faster than bigger (dist={})",
            small_dist,
            big_dist
        );
    }

    // ----------------------------------------------------------------
    //  to_game_state
    // ----------------------------------------------------------------

    #[test]
    fn test_to_game_state_excludes_dead_players() {
        let mut p1 = Player::new("p1".into(), 500.0, 400.0, "#ff0000".into());
        p1.size = 50.0;
        let mut p2 = Player::new("p2".into(), 503.0, 400.0, "#00ff00".into());
        p2.size = 10.0;
        let players = vec![p1, p2];
        let mut session = make_session(players, vec![], 1000, 800);

        // Kill p2
        run_ticks(&mut session, 1, 1.0 / 30.0);

        let state = session.to_game_state();
        assert_eq!(state.players.len(), 1, "Only alive players should appear in game state");
        assert_eq!(state.players[0].id, "p1", "Surviving player should be p1");
    }

    #[test]
    fn test_to_game_state_includes_all_food() {
        let player = Player::new("p1".into(), 100.0, 100.0, "#ff0000".into());
        let food = vec![
            Food {
                id: 0,
                x: 200.0,
                y: 200.0,
                size: 6.0,
            },
            Food {
                id: 1,
                x: 300.0,
                y: 300.0,
                size: 7.0,
            },
        ];
        let session = make_session(vec![player], food, 1000, 800);
        let state = session.to_game_state();
        assert_eq!(state.food.len(), 2, "Game state should include all food items");
    }
}