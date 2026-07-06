//! Direction → room bounds / entity coords using layout context.

use aether_package::{
    attach_room, default_attach_height, default_attach_width, default_main_room_bounds,
    extremal_room_bounds, place_in_room, room_is_beyond, Direction, LayoutContext, RoomAnchor,
    RoomBoundsDef,
};
use serde_json::{json, Value};

pub fn normalize_direction(s: &str) -> &'static str {
    Direction::parse(s)
        .map(|d| match d {
            Direction::North => "north",
            Direction::South => "south",
            Direction::East => "east",
            Direction::West => "west",
        })
        .unwrap_or("center")
}

fn attach_parent(layout: &LayoutContext, scene_id: &str, dir: Direction) -> RoomBoundsDef {
    let anchor = reference_bounds(layout, scene_id);
    let existing = layout.bounds_for_scene();
    extremal_room_bounds(&existing, &anchor, dir)
}

/// Bounds for a new room attached beyond existing rooms in `direction`.
pub fn room_bounds_for_direction(
    layout: &LayoutContext,
    scene_id: &str,
    direction: &str,
) -> (&'static str, RoomBoundsDef) {
    let suffix = normalize_direction(direction);
    let Some(dir) = Direction::parse(direction) else {
        return (
            "center",
            RoomBoundsDef {
                x: -120.0,
                y: -60.0,
                width: 240.0,
                height: 140.0,
            },
        );
    };

    let parent = attach_parent(layout, scene_id, dir);
    let w = default_attach_width(dir);
    let h = default_attach_height(dir);
    let bounds = attach_room(&parent, dir, w, h);
    (suffix, bounds)
}

/// World coords for a collectible placed toward `direction` from the reference layout.
pub fn coords_for_direction(
    layout: &LayoutContext,
    scene_id: &str,
    direction: &str,
) -> (&'static str, f32, f32) {
    let suffix = normalize_direction(direction);
    let Some(dir) = Direction::parse(direction) else {
        return ("center", 80.0, 100.0);
    };

    let anchor = reference_bounds(layout, scene_id);
    let tip = attach_parent(layout, scene_id, dir);
    let placement_room = if room_is_beyond(&tip, &anchor, dir) {
        tip
    } else {
        attach_room(
            &tip,
            dir,
            default_attach_width(dir),
            default_attach_height(dir),
        )
    };
    let (x, y) = place_in_room(&placement_room, RoomAnchor::for_direction(dir));
    (suffix, x, y)
}

fn reference_bounds(layout: &LayoutContext, scene_id: &str) -> RoomBoundsDef {
    let _ = scene_id;
    layout
        .bounds_for_scene()
        .into_iter()
        .max_by(|a, b| {
            a.area()
                .partial_cmp(&b.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_else(default_main_room_bounds)
}

pub fn bounds_to_json(b: &RoomBoundsDef) -> Value {
    json!({
        "x": b.x,
        "y": b.y,
        "width": b.width,
        "height": b.height
    })
}

pub fn simple_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{n:x}")
}
