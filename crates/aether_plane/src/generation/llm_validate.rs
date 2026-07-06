//! Extra guardrails for LLM-produced incremental patches (not simulated / game bootstrap).

use std::collections::HashSet;

use aether_core::{EngineError, EngineResult};
use aether_package::GamePackage;
use aether_patch::PatchDocument;

const MAX_COORD: f32 = 400.0;

/// Reject patches that violate incremental-edit conventions.
pub fn validate_llm_patch(
    patch: &PatchDocument,
    scene_id: &str,
    canonical: &GamePackage,
) -> EngineResult<Vec<String>> {
    let mut notes = Vec::new();
    let existing_ids: HashSet<&str> = canonical.entities.iter().map(|e| e.id.as_str()).collect();
    let collectible_owner: std::collections::HashMap<&str, &str> = canonical
        .entities
        .iter()
        .filter_map(|e| {
            e.components
                .collectible
                .as_ref()
                .map(|c| (c.id.as_str(), e.id.as_str()))
        })
        .collect();
    let mut patch_entity_ids = HashSet::new();
    let mut patch_collectible_ids = HashSet::new();
    let mut patch_room_ids: HashSet<String> =
        canonical.rooms.iter().map(|r| r.id.clone()).collect();
    for op in &patch.ops {
        if let Some(room_op) = aether_patch::parse_upsert_room(op) {
            if let Ok(room) = serde_json::from_value::<aether_package::RoomDef>(room_op.room.clone())
            {
                patch_room_ids.insert(room.id);
            }
        }
    }

    for op in &patch.ops {
        if let Some(entity_op) = aether_patch::parse_upsert_entity(op) {
            validate_entity_op(
                &entity_op,
                scene_id,
                &existing_ids,
                &collectible_owner,
                &mut patch_entity_ids,
                &mut patch_collectible_ids,
                &mut notes,
            )?;
        } else if let Some(room_op) = aether_patch::parse_upsert_room(op) {
            validate_room_op(&room_op, scene_id)?;
        } else if let Some(paint_op) = aether_patch::parse_paint_tiles(op) {
            validate_paint_op(&paint_op, scene_id, &patch_room_ids)?;
        }
    }

    Ok(notes)
}

fn validate_entity_op(
    op: &aether_patch::UpsertEntityOp,
    scene_id: &str,
    existing_ids: &HashSet<&str>,
    collectible_owner: &std::collections::HashMap<&str, &str>,
    patch_entity_ids: &mut HashSet<String>,
    patch_collectible_ids: &mut HashSet<String>,
    notes: &mut Vec<String>,
) -> EngineResult<()> {
    if op.scene != scene_id {
        return Err(EngineError::PatchRejected(format!(
            "entity op scene '{}' must match active scene '{}'",
            op.scene, scene_id
        )));
    }

    let entity = &op.entity;
    let id = entity
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EngineError::PatchRejected("upsert_entity missing entity.id".into()))?;

    if !patch_entity_ids.insert(id.to_string()) {
        return Err(EngineError::PatchRejected(format!(
            "duplicate entity id '{id}' in same patch"
        )));
    }

    if id == "player" || id == "win" {
        return Err(EngineError::PatchRejected(format!(
            "cannot upsert core entity '{id}' via LLM patch — edit canonical or regenerate game"
        )));
    }

    let components = entity
        .get("components")
        .ok_or_else(|| EngineError::PatchRejected(format!("entity '{id}' missing components")))?;

    if components.get("sprite").is_some() {
        return Err(EngineError::PatchRejected(
            "LLM patches must not include sprite — use generate-art or generate-world for visuals"
                .into(),
        ));
    }

    if let Some(t) = components.get("transform") {
        let x = t.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        let y = t.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0) as f32;
        if x.abs() > MAX_COORD || y.abs() > MAX_COORD {
            return Err(EngineError::PatchRejected(format!(
                "transform ({x}, {y}) out of bounds (max ±{MAX_COORD})"
            )));
        }
    }

    if let Some(c) = components.get("collectible") {
        let cid = c
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if cid.trim().is_empty() {
            return Err(EngineError::PatchRejected(format!(
                "entity '{id}' has empty collectible.id"
            )));
        }
        if !patch_collectible_ids.insert(cid.to_string()) {
            return Err(EngineError::PatchRejected(format!(
                "duplicate collectible.id '{cid}' in same patch"
            )));
        }
        if let Some(owner) = collectible_owner.get(cid) {
            if *owner != id {
                return Err(EngineError::PatchRejected(format!(
                    "collectible.id '{cid}' already used by entity '{owner}' — use a unique id"
                )));
            }
        }
    }

    if existing_ids.contains(id) {
        notes.push(format!("upsert overwrites existing entity '{id}'"));
    }

    Ok(())
}

fn validate_paint_op(
    op: &aether_patch::PaintTilesOp,
    scene_id: &str,
    room_ids: &HashSet<String>,
) -> EngineResult<()> {
    if op.scene != scene_id {
        return Err(EngineError::PatchRejected(format!(
            "paint_tiles scene '{}' must match active scene '{}'",
            op.scene, scene_id
        )));
    }
    if !room_ids.contains(&op.room_id) {
        return Err(EngineError::PatchRejected(format!(
            "paint_tiles room '{}' does not exist in canonical or this patch",
            op.room_id
        )));
    }
    let has_fill = op.fill.as_ref().is_some_and(|s| !s.trim().is_empty());
    let has_cells = op
        .cells
        .as_ref()
        .and_then(|v| v.as_array())
        .is_some_and(|a| !a.is_empty());
    if !has_fill && !has_cells {
        return Err(EngineError::PatchRejected(
            "paint_tiles requires fill or non-empty cells".into(),
        ));
    }
    Ok(())
}

fn validate_room_op(op: &aether_patch::UpsertRoomOp, scene_id: &str) -> EngineResult<()> {
    if op.scene != scene_id {
        return Err(EngineError::PatchRejected(format!(
            "room op scene '{}' must match active scene '{}'",
            op.scene, scene_id
        )));
    }
    let room = &op.room;
    let bounds = room
        .get("bounds")
        .ok_or_else(|| EngineError::PatchRejected("upsert_room missing bounds".into()))?;
    let w = bounds.get("width").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let h = bounds.get("height").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return Err(EngineError::PatchRejected(
            "room bounds width and height must be positive".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_package::load_package;

    fn example_package() -> GamePackage {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        load_package(path).expect("load")
    }

    #[test]
    fn rejects_sprite_in_llm_patch() {
        let patch = PatchDocument {
            patch_id: "t".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "upsert_entity",
                "scene": "main",
                "entity": {
                    "id": "bad",
                    "scene": "main",
                    "components": {
                        "sprite": { "asset": "fake" },
                        "transform": { "x": 0, "y": 0 }
                    }
                }
            })],
        };
        let pkg = example_package();
        let err = validate_llm_patch(&patch, "main", &pkg).unwrap_err();
        assert!(err.to_string().contains("sprite"));
    }
}
