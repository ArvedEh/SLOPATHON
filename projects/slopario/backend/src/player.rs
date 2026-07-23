use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub color: String,
    pub direction: Option<Direction>,
    pub alive: bool,
    pub score: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Direction {
    pub x: f64,
    pub y: f64,
}

impl Player {
    pub fn new(id: String, x: f64, y: f64, color: String) -> Self {
        Self {
            id,
            x,
            y,
            size: 20.0,
            color,
            direction: None,
            alive: true,
            score: 0,
        }
    }
}