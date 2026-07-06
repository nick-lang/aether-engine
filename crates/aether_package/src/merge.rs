//! Apply live patch ops into an in-memory [`GamePackage`] (canonical merge).

use std::collections::HashSet;

use aether_core::EngineResult;
use aether_patch::PatchDocument;

use crate::{EntityDef, GamePackage, RoomDef, TileCellDef, TileLayerDef};
use crate::validate::{validate_package, validate_room};

/// True when this patch cannot be applied to `package` (e.g. paint on a missing room).
pub fn patch_invalid_for_package(patch: &PatchDocument, package: &GamePackage) -> bool {
    let room_ids: HashSet<&str> = package.rooms.iter().map(|r| r.id.as_str()).collect();
    let entity_ids: HashSet<&str> = package.entities.iter().map(|e| e.id.as_str()).collect();

    for op in patch.upsert_room_ops() {
        let Ok(room) = serde_json::from_value::<RoomDef>(op.room) else {
            return true;
        };
        if !room_ids.contains(room.id.as_str()) {
            return true;
        }
    }
    for op in patch.paint_tiles_ops() {
        if !room_ids.contains(op.room_id.as_str()) {
            return true;
        }
    }
    for op in patch.upsert_entity_ops() {
        let Ok(entity) = serde_json::from_value::<EntityDef>(op.entity) else {
            return true;
        };
        if !entity_ids.contains(entity.id.as_str()) {
            return true;
        }
    }
    false
}

/// Drop tile layers whose `room_id` is not present in the package.
pub fn prune_orphan_tile_layers(package: &mut GamePackage) {
    let room_ids: HashSet<&str> = package.rooms.iter().map(|r| r.id.as_str()).collect();
    package
        .tile_layers
        .retain(|layer| room_ids.contains(layer.room_id.as_str()));
}

pub fn apply_patch_to_package(package: &mut GamePackage, patch: &PatchDocument) -> EngineResult<()> {
    for op in patch.upsert_room_ops() {
        let room: RoomDef = serde_json::from_value(op.room).map_err(|e| {
            aether_core::EngineError::PatchRejected(format!("invalid room in patch: {e}"))
        })?;
        if room.scene != op.scene {
            return Err(aether_core::EngineError::PatchRejected(format!(
                "room '{}' scene '{}' does not match op scene '{}'",
                room.id, room.scene, op.scene
            )));
        }
        upsert_room(package, room)?;
    }
    for op in patch.upsert_entity_ops() {
        let entity: EntityDef = serde_json::from_value(op.entity).map_err(|e| {
            aether_core::EngineError::PatchRejected(format!("invalid entity in patch: {e}"))
        })?;
        if entity.scene != op.scene {
            return Err(aether_core::EngineError::PatchRejected(format!(
                "entity '{}' scene '{}' does not match op scene '{}'",
                entity.id, entity.scene, op.scene
            )));
        }
        upsert_entity(package, entity);
    }
    for op in patch.paint_tiles_ops() {
        upsert_tile_layer(package, paint_op_to_layer(&op)?);
    }
    prune_orphan_tile_layers(package);
    validate_package(package)?;
    Ok(())
}

/// True when applying `patch` to `package` leaves it unchanged (no-op / rename-only).
pub fn patch_is_noop_for_package(
    package: &GamePackage,
    patch: &PatchDocument,
) -> EngineResult<bool> {
    let mut after = package.clone();
    apply_patch_to_package(&mut after, patch)?;
    let before = serde_json::to_value(package).map_err(|e| {
        aether_core::EngineError::InvalidPackage(format!("serialize package: {e}"))
    })?;
    let after_val = serde_json::to_value(&after).map_err(|e| {
        aether_core::EngineError::InvalidPackage(format!("serialize patched package: {e}"))
    })?;
    Ok(before == after_val)
}

fn upsert_room(package: &mut GamePackage, room: RoomDef) -> EngineResult<()> {
    let scene_ids: std::collections::HashSet<&str> =
        package.scenes.iter().map(|s| s.id.as_str()).collect();
    validate_room(&room, &scene_ids)?;
    if let Some(existing) = package.rooms.iter_mut().find(|r| r.id == room.id) {
        *existing = room;
    } else {
        package.rooms.push(room);
    }
    Ok(())
}

fn upsert_entity(package: &mut GamePackage, entity: EntityDef) {
    if let Some(existing) = package.entities.iter_mut().find(|e| e.id == entity.id) {
        *existing = entity;
    } else {
        package.entities.push(entity);
    }
}

pub fn paint_op_to_layer(op: &aether_patch::PaintTilesOp) -> EngineResult<TileLayerDef> {
    if let Some(fill) = op.fill.as_ref().filter(|s| !s.trim().is_empty()) {
        return Ok(TileLayerDef {
            scene: op.scene.clone(),
            room_id: op.room_id.clone(),
            tile_size: op.tile_size,
            fill: Some(fill.clone()),
            cells: Vec::new(),
        });
    }
    let cells_val = op.cells.as_ref().ok_or_else(|| {
        aether_core::EngineError::PatchRejected(
            "paint_tiles requires fill or cells".into(),
        )
    })?;
    let cells: Vec<TileCellDef> = serde_json::from_value(cells_val.clone()).map_err(|e| {
        aether_core::EngineError::PatchRejected(format!("invalid paint_tiles cells: {e}"))
    })?;
    if cells.is_empty() {
        return Err(aether_core::EngineError::PatchRejected(
            "paint_tiles cells must not be empty when fill is omitted".into(),
        ));
    }
    Ok(TileLayerDef {
        scene: op.scene.clone(),
        room_id: op.room_id.clone(),
        tile_size: op.tile_size,
        fill: None,
        cells,
    })
}

fn upsert_tile_layer(package: &mut GamePackage, layer: TileLayerDef) {
    let key = (layer.scene.as_str(), layer.room_id.as_str());
    if let Some(existing) = package.tile_layers.iter_mut().find(|l| {
        (l.scene.as_str(), l.room_id.as_str()) == key
    }) {
        *existing = layer;
    } else {
        package.tile_layers.push(layer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_patch::load_patch;

    #[test]
    fn merge_room_patch_into_package() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let mut package = crate::load_package(&path).expect("load");
        let patch_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/patches/add_north_room.json");
        let patch = load_patch(&patch_path).expect("patch");
        apply_patch_to_package(&mut package, &patch).expect("merge");
        assert!(package.rooms.iter().any(|r| r.id == "north_grove"));
    }

    #[test]
    fn merge_paint_fill_layer() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let mut package = crate::load_package(&path).expect("load");
        let patch = aether_patch::PatchDocument {
            patch_id: "paint_moss".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "paint_tiles",
                "scene": "main",
                "room_id": "starting_grove",
                "tile_size": 16.0,
                "fill": "moss"
            })],
        };
        apply_patch_to_package(&mut package, &patch).expect("merge");
        let layer = package
            .tile_layers
            .iter()
            .find(|l| l.room_id == "starting_grove")
            .expect("layer");
        assert_eq!(layer.fill.as_deref(), Some("moss"));
        assert!(layer.cells.is_empty());
    }

    #[test]
    fn detects_noop_room_reupsert() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let package = crate::load_package(&path).expect("load");
        let room = package
            .rooms
            .iter()
            .find(|r| r.id == "starting_grove")
            .expect("starting room")
            .clone();
        let patch = PatchDocument {
            patch_id: "noop_room".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "upsert_room",
                "scene": "main",
                "room": room,
            })],
        };
        assert!(patch_is_noop_for_package(&package, &patch).expect("check"));
    }

    #[test]
    fn detects_effective_paint_change() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let package = crate::load_package(&path).expect("load");
        let patch = PatchDocument {
            patch_id: "paint_moss".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "paint_tiles",
                "scene": "main",
                "room_id": "starting_grove",
                "tile_size": 16.0,
                "fill": "moss"
            })],
        };
        assert!(!patch_is_noop_for_package(&package, &patch).expect("check"));
    }
}
