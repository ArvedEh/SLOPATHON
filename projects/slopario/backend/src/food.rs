use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Food {
    pub id: u64,
    pub x: f64,
    pub y: f64,
    pub size: f64,
}

/// Erzeugt Mock-Food für Demo-Zwecke
pub fn create_mock_food() -> Vec<Food> {
    let count = 20;
    let mut food = Vec::with_capacity(count);
    for i in 0..count {
        food.push(Food {
            id: i as u64,
            x: rand::random::<f64>() * 900.0 + 50.0,
            y: rand::random::<f64>() * 700.0 + 50.0,
            size: 5.0 + rand::random::<f64>() * 5.0,
        });
    }
    food
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