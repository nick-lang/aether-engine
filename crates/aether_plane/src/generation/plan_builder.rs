//! Turn an [`IncrementalPlan`] into a validated incremental patch.

use std::collections::HashSet;

use aether_core::EngineResult;
use aether_package::{GamePackage, LayoutContext, RoomBoundsDef};
use aether_patch::PatchDocument;
use serde_json::json;

use super::incremental_plan::{validate_plan, IncrementalPlan, PlanAction};
use super::spatial::{
    bounds_to_json, coords_for_direction, normalize_direction, room_bounds_for_direction,
    simple_nonce,
};

pub fn plan_to_patch(
    plan: &IncrementalPlan,
    package_version: u64,
    scene_id: &str,
    canonical: &GamePackage,
    pending_patches: &[PatchDocument],
) -> EngineResult<PatchDocument> {
    validate_plan(plan)?;

    let mut layout = LayoutContext::from_canonical(canonical, scene_id);
    layout.apply_patches(pending_patches, scene_id)?;

    let mut used_rooms: HashSet<String> = canonical.rooms.iter().map(|r| r.id.clone()).collect();
    for patch in pending_patches {
        for op in patch.upsert_room_ops() {
            if let Ok(room) = serde_json::from_value::<aether_package::RoomDef>(op.room.clone()) {
                used_rooms.insert(room.id);
            }
        }
    }
    let mut used_entities: HashSet<String> =
        canonical.entities.iter().map(|e| e.id.clone()).collect();
    let mut used_collectible_ids: HashSet<String> = canonical
        .entities
        .iter()
        .filter_map(|e| e.components.collectible.as_ref().map(|c| c.id.clone()))
        .collect();

    let rooms_with_explicit_paint = planned_paint_room_ids(plan, &layout, scene_id, &used_rooms);

    let mut ops = Vec::new();
    let mut last_room_id: Option<String> = None;
    for action in &plan.actions {
        match action {
            PlanAction::AddRoom { direction, label } => {
                let dir = normalize_direction(direction);
                let (suffix, bounds) = room_bounds_for_direction(&layout, scene_id, dir);
                let base_id = format!("gen_room_{suffix}");
                let room_id = unique_id_owned(&base_id, &mut used_rooms);
                layout.push_room(room_id.clone(), bounds.clone());
                last_room_id = Some(room_id.clone());
                let room_label =
                    label.clone().unwrap_or_else(|| format!("Generated {suffix} room"));
                ops.push(json!({
                    "op": "upsert_room",
                    "scene": scene_id,
                    "room": {
                        "id": room_id,
                        "scene": scene_id,
                        "bounds": bounds_to_json(&bounds),
                        "label": room_label
                    }
                }));
                if !rooms_with_explicit_paint.contains(&room_id) {
                    let fill = default_fill_for_room(&suffix, &room_label);
                    ops.push(json!({
                        "op": "paint_tiles",
                        "scene": scene_id,
                        "room_id": room_id,
                        "tile_size": 16.0,
                        "fill": fill
                    }));
                }
            }
            PlanAction::AddCollectible {
                direction,
                id,
                label: _,
            } => {
                let dir = normalize_direction(direction);
                let (suffix, x, y) = coords_for_direction(&layout, scene_id, dir);
                let base_collectible = id
                    .as_ref()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| sanitize_id(s))
                    .unwrap_or_else(|| format!("gen_shard_{suffix}"));
                let collectible_id =
                    unique_id_owned(&base_collectible, &mut used_collectible_ids);
                let base_entity = format!("pickup_{}", sanitize_id(&collectible_id));
                let entity_id = unique_id_owned(&base_entity, &mut used_entities);
                ops.push(json!({
                    "op": "upsert_entity",
                    "scene": scene_id,
                    "entity": {
                        "id": entity_id,
                        "scene": scene_id,
                        "components": {
                            "transform": { "x": x, "y": y },
                            "collectible": { "id": collectible_id }
                        }
                    }
                }));
            }
            PlanAction::PaintTiles {
                direction,
                tile,
                room_id,
            } => {
                let tile_name = tile.trim();
                if tile_name.is_empty() {
                    continue;
                }
                let paint_room = room_id
                    .as_deref()
                    .or(last_room_id.as_deref());
                let (target_room_id, _room_bounds) =
                    resolve_paint_target(&layout, direction, paint_room);
                ops.push(json!({
                    "op": "paint_tiles",
                    "scene": scene_id,
                    "room_id": target_room_id,
                    "tile_size": 16.0,
                    "fill": tile_name
                }));
            }
        }
    }

    let patch_id = format!("plan_{}", simple_nonce());
    Ok(PatchDocument {
        patch_id,
        base_version: package_version,
        ops,
    })
}

fn resolve_paint_target(
    layout: &LayoutContext,
    direction: &str,
    room_id: Option<&str>,
) -> (String, RoomBoundsDef) {
    if let Some(id) = room_id {
        if let Some(b) = layout.find_room_bounds(id) {
            return (id.to_string(), b.clone());
        }
    }
    if let Some(dir) = aether_package::Direction::parse(direction) {
        if let Some((id, bounds)) = layout.extremal_room_in_direction(dir) {
            return (id, bounds);
        }
    }
    let anchor = layout
        .bounds_for_scene()
        .into_iter()
        .max_by(|a, b| {
            a.area()
                .partial_cmp(&b.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_else(aether_package::default_main_room_bounds);
    (
        room_id
            .unwrap_or("starting_grove")
            .to_string(),
        anchor,
    )
}

fn unique_id_owned(base: &str, taken: &mut HashSet<String>) -> String {
    if taken.insert(base.to_string()) {
        return base.to_string();
    }
    let id = format!("{base}_{}", simple_nonce());
    taken.insert(id.clone());
    id
}

/// Rooms that receive an explicit `PaintTiles` action in the plan (skip auto-floor on AddRoom).
fn planned_paint_room_ids(
    plan: &IncrementalPlan,
    layout: &LayoutContext,
    scene_id: &str,
    used_rooms: &HashSet<String>,
) -> HashSet<String> {
    let mut sim = layout.clone();
    let mut dry_used = used_rooms.clone();
    let mut painted = HashSet::new();
    for action in &plan.actions {
        match action {
            PlanAction::AddRoom { direction, label: _ } => {
                let dir = normalize_direction(direction);
                let (suffix, bounds) = room_bounds_for_direction(&sim, scene_id, dir);
                let base_id = format!("gen_room_{suffix}");
                let room_id = unique_id_owned(&base_id, &mut dry_used);
                sim.push_room(room_id, bounds);
            }
            PlanAction::PaintTiles {
                direction,
                tile,
                room_id,
            } => {
                if tile.trim().is_empty() {
                    continue;
                }
                let (target_room_id, _) =
                    resolve_paint_target(&sim, direction, room_id.as_deref());
                painted.insert(target_room_id);
            }
            PlanAction::AddCollectible { .. } => {}
        }
    }
    painted
}

/// Default floor when a generated room is added (visual contrast north vs main).
fn default_fill_for_room(suffix: &str, label: &str) -> &'static str {
    let l = label.to_lowercase();
    if l.contains("moss") {
        return "moss";
    }
    if l.contains("dungeon") || l.contains("stone") || l.contains("cave") {
        return "wall";
    }
    match suffix {
        "north" => "moss",
        "south" | "east" | "west" => "grass",
        _ => "grass",
    }
}

fn sanitize_id(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_package::load_package;
    use aether_patch::load_patch;
    use crate::generation::incremental_plan::{parse_plan_from_keywords, PlanAction};

    #[test]
    fn builds_room_patch() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = load_package(path).expect("load");
        let plan = IncrementalPlan {
            actions: vec![PlanAction::AddRoom {
                direction: "north".into(),
                label: Some("North Grove".into()),
            }],
        };
        let patch = plan_to_patch(&plan, 1, "main", &canonical, &[]).expect("patch");
        let room = patch.upsert_room_ops().next().unwrap();
        let bounds = room.room.get("bounds").expect("bounds");
        assert_eq!(bounds.get("x").and_then(|v| v.as_f64()), Some(-220.0));
        assert_eq!(bounds.get("y").and_then(|v| v.as_f64()), Some(140.0));
        let paint = patch
            .paint_tiles_ops()
            .find(|p| p.fill.as_deref() == Some("moss"))
            .expect("north room auto-painted with moss");
        assert!(paint.room_id.starts_with("gen_room_north"));
    }

    #[test]
    fn two_north_rooms_in_one_plan_chain() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = load_package(path).expect("load");
        let plan = IncrementalPlan {
            actions: vec![
                PlanAction::AddRoom {
                    direction: "north".into(),
                    label: None,
                },
                PlanAction::AddRoom {
                    direction: "north".into(),
                    label: None,
                },
            ],
        };
        let patch = plan_to_patch(&plan, 1, "main", &canonical, &[]).expect("patch");
        let ys: Vec<f64> = patch
            .upsert_room_ops()
            .filter_map(|op| {
                op.room
                    .get("bounds")?
                    .get("y")?
                    .as_f64()
            })
            .collect();
        assert_eq!(ys, vec![140.0, 240.0]);
    }

    #[test]
    fn pending_north_room_shifts_next_generate() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = load_package(path).expect("load");
        let pending_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/patches/add_north_room.json");
        let pending = vec![load_patch(pending_path).expect("patch")];
        let plan = IncrementalPlan {
            actions: vec![PlanAction::AddRoom {
                direction: "north".into(),
                label: None,
            }],
        };
        let patch = plan_to_patch(&plan, 1, "main", &canonical, &pending).expect("patch");
        let y = patch
            .upsert_room_ops()
            .next()
            .unwrap()
            .room
            .get("bounds")
            .and_then(|b| b.get("y"))
            .and_then(|v| v.as_f64());
        assert_eq!(y, Some(240.0));
    }

    #[test]
    fn moss_floor_paints_existing_north_room() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let mut canonical = load_package(path).expect("load");
        let north_bounds = room_bounds_for_direction(
            &LayoutContext::from_canonical(&canonical, "main"),
            "main",
            "north",
        )
        .1;
        canonical.rooms.push(aether_package::RoomDef {
            id: "north_grove".into(),
            scene: "main".into(),
            bounds: north_bounds,
            label: Some("North".into()),
        });
        let plan = parse_plan_from_keywords("mossy floor in the north room").expect("plan");
        let patch = plan_to_patch(&plan, 1, "main", &canonical, &[]).expect("patch");
        assert_eq!(patch.upsert_room_ops().count(), 0);
        let paint = patch.paint_tiles_ops().next().expect("paint");
        assert_eq!(paint.room_id, "north_grove");
        assert_eq!(paint.fill.as_deref(), Some("moss"));
    }

    #[test]
    fn keyword_plan_builds() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = load_package(path).expect("load");
        let plan = parse_plan_from_keywords("add a shard to the east").expect("plan");
        let patch = plan_to_patch(&plan, 1, "main", &canonical, &[]).expect("patch");
        assert_eq!(patch.upsert_entity_ops().count(), 1);
    }
}
