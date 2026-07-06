//! **Dream** generation — model authors a full [`PatchDocument`]; Rust validates and repairs.

use aether_core::{EngineError, EngineResult};
use aether_package::{
    attach_room, default_attach_height, default_attach_width, default_main_room_bounds,
    extremal_room_bounds, Direction, GamePackage, RoomBoundsDef, RoomDef, TileCellDef,
};
use aether_patch::{parse_paint_tiles, parse_upsert_room, PatchDocument};
use serde_json::{json, Value};

use super::llm_validate::validate_llm_patch;
use super::ollama::{
    extract_message_content, format_ollama_http_error, http_client, map_ollama_request_error,
    ollama_model, ollama_url, LlmPatchResult,
};
use super::prompts::{dream_user_message, DREAM_SYSTEM_PROMPT};
use super::repair::parse_patch_json;
use super::rules::validate_patch_rules_dream;
use super::spatial::simple_nonce;
use super::trace::GenerationTrace;

const SNAP_GRID: f32 = 8.0;

/// Ollama → creative full patch (no keyword fallback, no plan builder).
pub fn generate_dream_patch(
    prompt: &str,
    package_version: u64,
    scene_id: &str,
    canonical_summary: Option<&str>,
    canonical: &GamePackage,
) -> EngineResult<LlmPatchResult> {
    let user = dream_user_message(package_version, scene_id, prompt, canonical_summary);
    match finish_dream_from_user_message(&user, package_version, scene_id, canonical) {
        Ok(result) => Ok(result),
        Err(first_err) => {
            let retry_user = format!(
                "{user}\n\nYour previous PatchDocument was rejected:\n{first_err}\n\n\
                 Output ONE corrected PatchDocument JSON only. Fix every issue listed."
            );
            let mut result = finish_dream_from_user_message(
                &retry_user,
                package_version,
                scene_id,
                canonical,
            )?;
            result
                .trace
                .validation_notes
                .push("dream: retried after validation failure".into());
            Ok(result)
        }
    }
}

fn finish_dream_from_user_message(
    user: &str,
    package_version: u64,
    scene_id: &str,
    canonical: &GamePackage,
) -> EngineResult<LlmPatchResult> {
    let base = ollama_url();
    let model = ollama_model();

    let body = json!({
        "model": model,
        "stream": false,
        "format": "json",
        "messages": [
            { "role": "system", "content": DREAM_SYSTEM_PROMPT },
            { "role": "user", "content": user }
        ]
    });

    let client = http_client()?;
    let url = format!("{}/api/chat", base.trim_end_matches('/'));
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(map_ollama_request_error)?;

    let status = res.status();
    let text = res
        .text()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    if !status.is_success() {
        return Err(EngineError::InvalidPackage(format_ollama_http_error(
            status, &text, &model,
        )));
    }

    let content = extract_message_content(&text)?;
    let mut trace = GenerationTrace::new("ollama_dream", Some(model), user.to_string(), content.clone());

    let mut patch = parse_patch_json(&content).map_err(|e| {
        trace.parse_error = Some(e.to_string());
        e
    })?;
    patch.base_version = package_version;

    let repair_notes = interpret_dream_patch(&mut patch, scene_id, canonical)?;
    trace
        .validation_notes
        .extend(repair_notes.iter().map(|n| format!("repair: {n}")));

    validate_patch_rules_dream(&patch).map_err(|e| {
        trace.validation_notes.push(format!("rules: {e}"));
        e
    })?;
    validate_llm_patch(&patch, scene_id, canonical).map_err(|e| {
        trace.validation_notes.push(format!("llm_validate: {e}"));
        e
    })
    .map(|notes| trace.validation_notes.extend(notes))?;

    trace.built_summary = Some(summarize_dream_patch(&patch));

    Ok(LlmPatchResult { patch, trace })
}

fn summarize_dream_patch(patch: &PatchDocument) -> String {
    let rooms = patch.upsert_room_ops().count();
    let entities = patch.upsert_entity_ops().count();
    let paints = patch.paint_tiles_ops().count();
    format!("dream: {rooms} room(s), {entities} entity op(s), {paints} paint op(s)")
}

/// Normalize model output without replacing creative intent (snap grid, compact paint, overlap repair).
pub fn interpret_dream_patch(
    patch: &mut PatchDocument,
    scene_id: &str,
    canonical: &GamePackage,
) -> EngineResult<Vec<String>> {
    let mut notes = Vec::new();
    if patch.patch_id.trim().is_empty() {
        patch.patch_id = format!("dream_{}", simple_nonce());
        notes.push("assigned patch_id".into());
    }

    let mut ops = Vec::with_capacity(patch.ops.len());
    for op in patch.ops.drain(..) {
        if parse_upsert_room(&op).is_some() {
            ops.push(snap_room_op(op)?);
        } else if parse_paint_tiles(&op).is_some() {
            ops.push(coerce_paint_fill_op(op));
        } else {
            ops.push(op);
        }
    }
    patch.ops = ops;
    notes.extend(repair_dream_room_overlaps(patch, scene_id, canonical));
    Ok(notes)
}

/// Reposition new rooms that stack on canonical (Dream often emits duplicate coords).
fn repair_dream_room_overlaps(
    patch: &mut PatchDocument,
    scene_id: &str,
    canonical: &GamePackage,
) -> Vec<String> {
    let mut notes = Vec::new();
    let mut layout: Vec<(String, RoomBoundsDef)> = canonical
        .rooms
        .iter()
        .filter(|r| r.scene == scene_id)
        .map(|r| (r.id.clone(), r.bounds.clone()))
        .collect();
    let anchor = layout
        .iter()
        .map(|(_, b)| b)
        .max_by(|a, b| {
            a.area()
                .partial_cmp(&b.area())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .cloned()
        .unwrap_or_else(default_main_room_bounds);

    for op in &mut patch.ops {
        let Some(room_op) = parse_upsert_room(op) else {
            continue;
        };
        let Ok(mut room) = serde_json::from_value::<RoomDef>(room_op.room.clone()) else {
            continue;
        };
        if room.scene != scene_id {
            continue;
        }

        let overlaps = layout
            .iter()
            .filter(|(id, _)| *id != room.id)
            .any(|(_, b)| intersection_area(&room.bounds, b) > 1.0);
        if overlaps {
            let dir = infer_room_direction(&room);
            let parent = extremal_room_bounds(
                &layout.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>(),
                &anchor,
                dir,
            );
            let w = room.bounds.width.max(default_attach_width(dir));
            let h = room.bounds.height.max(default_attach_height(dir));
            room.bounds = attach_room(&parent, dir, w, h);
            notes.push(format!(
                "repositioned room '{}' {dir:?} → ({:.0}, {:.0}) to avoid overlap",
                room.id, room.bounds.x, room.bounds.y
            ));
        }

        if let Some(entry) = layout.iter_mut().find(|(id, _)| *id == room.id) {
            entry.1 = room.bounds.clone();
        } else {
            layout.push((room.id.clone(), room.bounds.clone()));
        }

        *op = json!({
            "op": "upsert_room",
            "scene": room_op.scene,
            "room": room,
        });
    }
    notes
}

fn intersection_area(a: &RoomBoundsDef, b: &RoomBoundsDef) -> f32 {
    let w = a.max_x().min(b.max_x()) - a.min_x().max(b.min_x());
    let h = a.max_y().min(b.max_y()) - a.min_y().max(b.min_y());
    if w <= 0.0 || h <= 0.0 {
        0.0
    } else {
        w * h
    }
}

fn infer_room_direction(room: &RoomDef) -> Direction {
    let text = format!(
        "{} {}",
        room.id,
        room.label.as_deref().unwrap_or("")
    )
    .to_lowercase();
    if text.contains("north") {
        Direction::North
    } else if text.contains("south") {
        Direction::South
    } else if text.contains("east") {
        Direction::East
    } else if text.contains("west") {
        Direction::West
    } else {
        Direction::North
    }
}

fn snap_room_op(op: Value) -> EngineResult<Value> {
    let room_op = parse_upsert_room(&op).ok_or_else(|| {
        EngineError::PatchRejected("expected upsert_room".into())
    })?;
    let mut room: RoomDef = serde_json::from_value(room_op.room).map_err(|e| {
        EngineError::PatchRejected(format!("invalid room: {e}"))
    })?;
    room.bounds = snap_bounds(&room.bounds);
    Ok(json!({
        "op": "upsert_room",
        "scene": room_op.scene,
        "room": room,
    }))
}

fn snap_bounds(b: &RoomBoundsDef) -> RoomBoundsDef {
    let snap = |v: f32| (v / SNAP_GRID).round() * SNAP_GRID;
    let width = (b.width / SNAP_GRID).round().max(1.0) * SNAP_GRID;
    let height = (b.height / SNAP_GRID).round().max(1.0) * SNAP_GRID;
    RoomBoundsDef {
        x: snap(b.x),
        y: snap(b.y),
        width,
        height,
    }
}

/// Large homogeneous `cells` arrays become compact `fill` (keeps patch small).
fn coerce_paint_fill_op(op: Value) -> Value {
    let Some(paint) = parse_paint_tiles(&op) else {
        return op;
    };
    if paint.fill.is_some() {
        return op;
    }
    let Some(cells_val) = paint.cells else {
        return op;
    };
    let Ok(cells) = serde_json::from_value::<Vec<TileCellDef>>(cells_val) else {
        return op;
    };
    if cells.is_empty() {
        return op;
    }
    let first = cells[0].tile.as_str();
    if cells.iter().all(|c| c.tile == first) {
        return json!({
            "op": "paint_tiles",
            "scene": paint.scene,
            "room_id": paint.room_id,
            "tile_size": paint.tile_size,
            "fill": first,
        });
    }
    op
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coerce_cells_to_fill() {
        let op = json!({
            "op": "paint_tiles",
            "scene": "main",
            "room_id": "crypt",
            "tile_size": 16.0,
            "cells": [
                {"x": 0, "y": 0, "tile": "moss"},
                {"x": 1, "y": 0, "tile": "moss"}
            ]
        });
        let out = coerce_paint_fill_op(op);
        assert_eq!(out.get("fill").and_then(|v| v.as_str()), Some("moss"));
        assert!(out.get("cells").is_none());
    }

    #[test]
    fn snap_room_bounds_to_grid() {
        let op = json!({
            "op": "upsert_room",
            "scene": "main",
            "room": {
                "id": "weird",
                "scene": "main",
                "bounds": { "x": 3.2, "y": 7.1, "width": 103.0, "height": 99.0 }
            }
        });
        let out = snap_room_op(op).expect("snap");
        let b = &out["room"]["bounds"];
        assert_eq!(b["x"].as_f64(), Some(0.0));
        assert_eq!(b["y"].as_f64(), Some(8.0));
        assert_eq!(b["width"].as_f64(), Some(104.0));
    }

    #[test]
    fn repairs_stacked_room_onto_canonical() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = aether_package::load_package(&path).expect("load");
        let main = canonical
            .rooms
            .iter()
            .find(|r| r.id == "starting_grove")
            .expect("main room");
        let mut patch = PatchDocument {
            patch_id: "dream_test".into(),
            base_version: 1,
            ops: vec![json!({
                "op": "upsert_room",
                "scene": "main",
                "room": {
                    "id": "north_grove",
                    "scene": "main",
                    "bounds": main.bounds,
                    "label": "Northern Grove",
                }
            })],
        };
        let notes = interpret_dream_patch(&mut patch, "main", &canonical).expect("repair");
        assert!(notes.iter().any(|n| n.contains("repositioned")));
        let room = &patch.ops[0]["room"]["bounds"];
        assert!(room["y"].as_f64().unwrap() > main.bounds.max_y() as f64 - 1.0);
    }
}
