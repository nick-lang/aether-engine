//! World Director — gameplay events → patch intents (Phase 2 stub).

use aether_patch::PatchDocument;
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub struct GameplayEvent {
    pub event_type: String,
    #[serde(default)]
    pub payload: serde_json::Value,
}

/// Returns an optional auto-generated candidate patch for known event types.
pub fn patch_intent_from_event(
    package_version: u64,
    scene_id: &str,
    event: &GameplayEvent,
) -> Option<PatchDocument> {
    match event.event_type.as_str() {
        "spawn_bonus_shard" => Some(PatchDocument {
            patch_id: format!("director_{}", uuid_simple()),
            base_version: package_version,
            ops: vec![json!({
                "op": "upsert_entity",
                "scene": scene_id,
                "entity": {
                    "id": "director_bonus_shard",
                    "scene": scene_id,
                    "components": {
                        "transform": { "x": 0.0, "y": -160.0 },
                        "collectible": { "id": "director_bonus_shard" }
                    }
                }
            })],
        }),
        _ => None,
    }
}

fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}")
}
