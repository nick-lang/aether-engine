//! Combined world + art generation — one prompt, one patch candidate.

use std::path::Path;

use aether_core::EngineResult;
use aether_patch::PatchDocument;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::art::{generate_art_asset, simple_nonce};
use super::prompts;
use super::{generate_patch, validate_patch_rules, ArtMode, ArtPlacementMode, GenerateMode};
use aether_package::GamePackage;

#[derive(Debug, Deserialize)]
pub struct GenerateWorldRequest {
    pub prompt: String,
    #[serde(default)]
    pub patch_mode: GenerateMode,
    #[serde(default)]
    pub art_mode: ArtMode,
    #[serde(default)]
    pub placement: ArtPlacementMode,
    /// When unset, art runs if the prompt looks visual (sprite, chest, goblin, …).
    #[serde(default)]
    pub include_art: Option<bool>,
    #[serde(default = "super::default_true")]
    pub auto_submit: bool,
}

#[derive(Debug, Serialize)]
pub struct GenerateWorldResponse {
    pub patch: PatchDocument,
    pub patch_provider: &'static str,
    pub asset_id: Option<String>,
    pub art_provider: Option<&'static str>,
    pub placement: Option<&'static str>,
    pub art_included: bool,
    pub submitted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<super::GenerationTrace>,
}

pub fn generate_world(
    game_id: &str,
    prompt: &str,
    patch_mode: GenerateMode,
    art_mode: ArtMode,
    placement_mode: ArtPlacementMode,
    include_art: Option<bool>,
    scene_id: &str,
    package_version: u64,
    assets_dir: &Path,
    canonical: &GamePackage,
) -> EngineResult<GenerateWorldResponse> {
    let summary = prompts::canonical_summary(canonical);
    let outcome = generate_patch(
        prompt,
        patch_mode,
        package_version,
        scene_id,
        game_id,
        Some(summary.as_str()),
        canonical,
        &[],
    )?;
    let mut patch = outcome.patch;
    let trace = outcome.trace;
    let patch_provider = patch_provider_label(patch_mode);

    let should_art = include_art.unwrap_or_else(|| prompt_wants_art(prompt));
    let mut asset_id = None;
    let mut art_provider = None;
    let mut placement = None;

    if should_art {
        let entity_id = format!("art_prop_{}", simple_nonce());
        let (aid, op, provider, place) = generate_art_asset(
            prompt,
            art_mode,
            placement_mode,
            &entity_id,
            scene_id,
            assets_dir,
        )?;
        if !merge_art_into_patch(&mut patch, &aid, &op) {
            patch.ops.push(op);
        }
        patch.patch_id = format!("world_{}", simple_nonce());
        asset_id = Some(aid);
        art_provider = Some(provider);
        placement = Some(place);
    }

    validate_patch_rules(&patch)?;

    Ok(GenerateWorldResponse {
        patch,
        patch_provider,
        asset_id,
        art_provider,
        placement,
        art_included: should_art,
        submitted: false,
        trace,
    })
}

pub fn prompt_wants_art(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    [
        "sprite",
        "chest",
        "goblin",
        "prop",
        "portrait",
        "icon",
        "character",
        "npc",
        "enemy",
        "monster",
        "item",
        "treasure",
        "artifact",
        "visual",
        "png",
        "image",
        "art",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

/// Attach the generated PNG to an existing LLM `upsert_entity` with a sprite, or keep a new op.
/// Returns true when merged (no extra `upsert_entity` was appended).
fn merge_art_into_patch(patch: &mut PatchDocument, asset_id: &str, art_op: &Value) -> bool {
    let Some((x, y)) = op_transform_xy(art_op) else {
        return false;
    };

    let sprite_indices: Vec<usize> = patch
        .ops
        .iter()
        .enumerate()
        .filter_map(|(i, op)| op_has_sprite(op).then_some(i))
        .collect();

    if let Some(&idx) = sprite_indices.first() {
        set_op_sprite(&mut patch.ops[idx], asset_id, x, y);
        for i in sprite_indices.iter().skip(1).rev() {
            patch.ops.remove(*i);
        }
        return true;
    }
    false
}

fn op_has_sprite(op: &Value) -> bool {
    op.get("op").and_then(|v| v.as_str()) == Some("upsert_entity")
        && op
            .get("entity")
            .and_then(|e| e.get("components"))
            .and_then(|c| c.get("sprite"))
            .is_some()
}

fn op_transform_xy(op: &Value) -> Option<(f32, f32)> {
    let entity = op.get("entity")?;
    let t = entity.get("components")?.get("transform")?;
    let x = t.get("x")?.as_f64()? as f32;
    let y = t.get("y")?.as_f64()? as f32;
    Some((x, y))
}

fn set_op_sprite(op: &mut Value, asset_id: &str, x: f32, y: f32) {
    if let Some(entity) = op.get_mut("entity") {
        if let Some(components) = entity.get_mut("components") {
            if let Some(obj) = components.as_object_mut() {
                obj.insert(
                    "sprite".into(),
                    serde_json::json!({ "asset": asset_id }),
                );
                obj.insert("transform".into(), serde_json::json!({ "x": x, "y": y }));
            }
        }
    }
}

fn patch_provider_label(mode: GenerateMode) -> &'static str {
    match mode {
        GenerateMode::Simulated => "simulated",
        GenerateMode::Ollama => "ollama",
        GenerateMode::Dream => "ollama_dream",
        GenerateMode::OpenAi => "openai",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn art_heuristic_visual() {
        assert!(prompt_wants_art("goblin sprite to the east"));
        assert!(prompt_wants_art("rustic wooden chest"));
    }

    #[test]
    fn art_heuristic_room_only() {
        assert!(!prompt_wants_art("add a room to the north of the grove"));
    }

    #[test]
    fn merge_art_replaces_llm_sprite_and_drops_duplicate() {
        let mut patch = PatchDocument {
            patch_id: "test".into(),
            base_version: 1,
            ops: vec![
                serde_json::json!({
                    "op": "upsert_entity",
                    "scene": "main",
                    "entity": {
                        "id": "goblin_camp_entity",
                        "scene": "main",
                        "components": {
                            "sprite": { "asset": "goblin_sprite" },
                            "transform": { "x": 256, "y": 128 }
                        }
                    }
                }),
                serde_json::json!({
                    "op": "upsert_entity",
                    "scene": "main",
                    "entity": {
                        "id": "duplicate",
                        "scene": "main",
                        "components": {
                            "sprite": { "asset": "fake" },
                            "transform": { "x": 0, "y": 0 }
                        }
                    }
                }),
            ],
        };
        let art_op = crate::generation::art::create_sprite_op(
            "art_real123",
            "art_prop_x",
            "main",
            100.0,
            0.0,
        );
        assert!(merge_art_into_patch(&mut patch, "art_real123", &art_op));
        assert_eq!(patch.ops.len(), 1);
        let sprite = &patch.ops[0]["entity"]["components"]["sprite"]["asset"];
        assert_eq!(sprite.as_str(), Some("art_real123"));
    }
}
