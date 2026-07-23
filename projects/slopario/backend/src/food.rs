use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Food {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// Erzeugt ein Food-Item an einer zufälligen Position innerhalb der Map.
pub fn random_food(id: u64, map_width: u32, map_height: u32) -> Food {
    let margin = 10.0;
    Food {
        id,
        x: rand::random::<f64>() * (map_width as f64 - 2.0 * margin) + margin,
        y: rand::random::<f64>() * (map_height as f64 - 2.0 * margin) + margin,
        size: 5.0 + rand::random::<f64>() * 5.0,
    }
}