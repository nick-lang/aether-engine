//! Live patch types and op parsing (runtime apply in `aether_ecs`).

use aether_core::{EngineError, EngineResult, PackageVersion};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchDocument {
    pub patch_id: String,
    pub base_version: u64,
    pub ops: Vec<Value>,
}

#[derive(Debug, Clone)]
pub struct UpsertEntityOp {
    pub scene: String,
    pub entity: Value,
}

#[derive(Debug, Clone)]
pub struct UpsertRoomOp {
    pub scene: String,
    pub room: Value,
}

#[derive(Debug, Clone)]
pub struct PaintTilesOp {
    pub scene: String,
    pub room_id: String,
    pub tile_size: f32,
    /// Uniform room fill (`cells` omitted).
    pub fill: Option<String>,
    /// Legacy / sparse tile list (`fill` omitted).
    pub cells: Option<Value>,
}

impl PatchDocument {
    pub fn base_version(&self) -> PackageVersion {
        PackageVersion(self.base_version)
    }

    pub fn validate_version(&self, manifest_version: u64) -> EngineResult<()> {
        if self.base_version != manifest_version {
            return Err(EngineError::PatchRejected(format!(
                "patch {} expects base_version {} but manifest is {}",
                self.patch_id, self.base_version, manifest_version
            )));
        }
        Ok(())
    }

    pub fn upsert_entity_ops(&self) -> impl Iterator<Item = UpsertEntityOp> + '_ {
        self.ops.iter().filter_map(parse_upsert_entity)
    }

    pub fn upsert_room_ops(&self) -> impl Iterator<Item = UpsertRoomOp> + '_ {
        self.ops.iter().filter_map(parse_upsert_room)
    }

    pub fn paint_tiles_ops(&self) -> impl Iterator<Item = PaintTilesOp> + '_ {
        self.ops.iter().filter_map(parse_paint_tiles)
    }
}

pub fn parse_upsert_entity(op: &Value) -> Option<UpsertEntityOp> {
    if op.get("op")?.as_str()? != "upsert_entity" {
        return None;
    }
    Some(UpsertEntityOp {
        scene: op.get("scene")?.as_str()?.to_string(),
        entity: op.get("entity")?.clone(),
    })
}

pub fn parse_upsert_room(op: &Value) -> Option<UpsertRoomOp> {
    if op.get("op")?.as_str()? != "upsert_room" {
        return None;
    }
    Some(UpsertRoomOp {
        scene: op.get("scene")?.as_str()?.to_string(),
        room: op.get("room")?.clone(),
    })
}

pub fn parse_paint_tiles(op: &Value) -> Option<PaintTilesOp> {
    if op.get("op")?.as_str()? != "paint_tiles" {
        return None;
    }
    let fill = op
        .get("fill")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from);
    let cells = op.get("cells").cloned();
    if fill.is_none() && cells.is_none() {
        return None;
    }
    Some(PaintTilesOp {
        scene: op.get("scene")?.as_str()?.to_string(),
        room_id: op.get("room_id")?.as_str()?.to_string(),
        tile_size: op.get("tile_size").and_then(|v| v.as_f64()).unwrap_or(16.0) as f32,
        fill,
        cells,
    })
}

pub fn is_supported_op(op: &Value) -> bool {
    parse_upsert_entity(op).is_some()
        || parse_upsert_room(op).is_some()
        || parse_paint_tiles(op).is_some()
}

pub fn load_patch(path: impl AsRef<std::path::Path>) -> EngineResult<PatchDocument> {
    let text = std::fs::read_to_string(path.as_ref())
        .map_err(|e| EngineError::InvalidPackage(format!("read patch failed: {e}")))?;
    serde_json::from_str(&text)
        .map_err(|e| EngineError::InvalidPackage(format!("patch json parse failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_upsert_entity_op() {
        let op: Value = serde_json::json!({
            "op": "upsert_entity",
            "scene": "main",
            "entity": { "id": "x", "scene": "main", "components": {} }
        });
        let parsed = parse_upsert_entity(&op).expect("op");
        assert_eq!(parsed.scene, "main");
    }

    #[test]
    fn parses_upsert_room_op() {
        let op: Value = serde_json::json!({
            "op": "upsert_room",
            "scene": "main",
            "room": {
                "id": "north",
                "scene": "main",
                "bounds": { "x": 0, "y": 100, "width": 200, "height": 80 }
            }
        });
        let parsed = parse_upsert_room(&op).expect("op");
        assert_eq!(parsed.scene, "main");
    }

    #[test]
    fn example_patch_loads() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/patches/add_second_shard.json");
        let patch = load_patch(&path).expect("load patch");
        assert_eq!(patch.base_version, 1);
        assert_eq!(patch.upsert_entity_ops().count(), 1);
    }

    #[test]
    fn parses_paint_tiles_fill() {
        let op: Value = serde_json::json!({
            "op": "paint_tiles",
            "scene": "main",
            "room_id": "north_grove",
            "tile_size": 16.0,
            "fill": "moss"
        });
        let parsed = parse_paint_tiles(&op).expect("op");
        assert_eq!(parsed.fill.as_deref(), Some("moss"));
        assert!(parsed.cells.is_none());
    }

    #[test]
    fn example_room_patch_loads() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/patches/add_north_room.json");
        let patch = load_patch(&path).expect("load patch");
        assert_eq!(patch.upsert_room_ops().count(), 1);
    }
}
