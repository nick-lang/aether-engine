//! Tile grid helpers for room-local paint ops.

use crate::{RoomBoundsDef, TileCellDef};

/// One drawn tile quad (full or edge-clipped) in world space.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedTile {
    pub tile: String,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
}

/// Cover `room.bounds` with `tile` at fixed `tile_size`, clipping partial tiles at edges.
pub fn tessellate_room_fill(
    room: &RoomBoundsDef,
    tile: &str,
    tile_size: f32,
) -> Vec<PlacedTile> {
    let ts = tile_size.max(4.0);
    let rx0 = room.x;
    let ry0 = room.y;
    let rx1 = room.x + room.width;
    let ry1 = room.y + room.height;
    let mut out = Vec::new();
    let mut cy = ry0;
    while cy < ry1 - 1e-4 {
        let cell_h = (ry1 - cy).min(ts);
        let mut cx = rx0;
        while cx < rx1 - 1e-4 {
            let cell_w = (rx1 - cx).min(ts);
            out.push(PlacedTile {
                tile: tile.to_string(),
                center_x: cx + cell_w * 0.5,
                center_y: cy + cell_h * 0.5,
                width: cell_w,
                height: cell_h,
            });
            cx += ts;
        }
        cy += ts;
    }
    out
}

/// Legacy: explicit cell grid (may leave gaps at room edges when bounds are not tile-aligned).
pub fn fill_room_grid(room: &RoomBoundsDef, tile: &str, tile_size: f32) -> Vec<TileCellDef> {
    let tile_size = tile_size.max(4.0);
    let cols = (room.width / tile_size).floor().max(1.0) as i32;
    let rows = (room.height / tile_size).floor().max(1.0) as i32;
    let mut cells = Vec::with_capacity((cols * rows) as usize);
    for y in 0..rows {
        for x in 0..cols {
            cells.push(TileCellDef {
                x,
                y,
                tile: tile.to_string(),
            });
        }
    }
    cells
}

pub fn tile_world_center(
    room: &RoomBoundsDef,
    cell_x: i32,
    cell_y: i32,
    tile_size: f32,
) -> (f32, f32) {
    let wx = room.x + (cell_x as f32 + 0.5) * tile_size;
    let wy = room.y + (cell_y as f32 + 0.5) * tile_size;
    (wx, wy)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tessellate_covers_remainder_on_east_and_north() {
        let room = RoomBoundsDef {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        };
        let placed = tessellate_room_fill(&room, "moss", 16.0);
        assert!(!placed.is_empty());
        let max_x = placed
            .iter()
            .map(|p| p.center_x + p.width * 0.5)
            .fold(f32::NEG_INFINITY, f32::max);
        let max_y = placed
            .iter()
            .map(|p| p.center_y + p.height * 0.5)
            .fold(f32::NEG_INFINITY, f32::max);
        assert!((max_x - 100.0).abs() < 0.01, "east edge covered, got {max_x}");
        assert!((max_y - 100.0).abs() < 0.01, "north edge covered, got {max_y}");
        assert!(
            placed.iter().any(|p| p.width < 16.0),
            "expects clipped edge column"
        );
        assert!(
            placed.iter().any(|p| p.height < 16.0),
            "expects clipped edge row"
        );
    }

    #[test]
    fn tessellate_interior_uses_full_tile_size() {
        let room = RoomBoundsDef {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
        };
        let placed = tessellate_room_fill(&room, "grass", 16.0);
        assert_eq!(placed.len(), 16);
        assert!(placed.iter().all(|p| (p.width - 16.0).abs() < 0.01 && (p.height - 16.0).abs() < 0.01));
    }
}
