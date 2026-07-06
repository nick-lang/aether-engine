//! Tile id → appearance (colors today; PNG catalog later).

/// RGB in 0..1 for procedural / placeholder tiles.
pub fn tile_rgb(tile: &str) -> [f32; 3] {
    match tile {
        "grass" => [0.35, 0.72, 0.32],
        "moss" => [0.55, 0.78, 0.38],
        "wall" => [0.45, 0.42, 0.5],
        "floor" => [0.55, 0.48, 0.38],
        "crystal" | "stone" => [0.62, 0.75, 0.88],
        "water" => [0.28, 0.52, 0.78],
        _ => [0.55, 0.48, 0.38],
    }
}

/// Future: asset path relative to plane assets dir when PNG catalog is wired.
pub fn tile_asset_id(tile: &str) -> Option<&'static str> {
    match tile {
        "grass" | "moss" | "floor" | "wall" => None,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_tiles_have_distinct_colors() {
        assert_ne!(tile_rgb("grass"), tile_rgb("moss"));
    }
}
