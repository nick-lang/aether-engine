//! Art worker — PNG asset + upsert_entity patch (stub, Ollama, or Automatic1111).

use std::path::Path;

use aether_core::EngineResult;
use aether_patch::PatchDocument;
use serde::{Deserialize, Serialize};
use serde_json::json;

use super::art_backends;
use super::art_placement::{self, ArtPlacementMode};
use super::ArtMode;

#[derive(Debug, Deserialize)]
pub struct GenerateArtRequest {
    pub prompt: String,
    #[serde(default)]
    pub mode: ArtMode,
    #[serde(default = "default_entity")]
    pub entity_id: String,
    #[serde(default = "default_scene")]
    pub scene: String,
    #[serde(default)]
    pub auto_submit: bool,
    /// How to pick `transform` for the sprite (`auto` tries Ollama when configured).
    #[serde(default)]
    pub placement: ArtPlacementMode,
}

fn default_entity() -> String {
    "prop_sprite".into()
}

fn default_scene() -> String {
    "main".into()
}

#[derive(Debug, Serialize)]
pub struct GenerateArtResponse {
    pub asset_id: String,
    pub patch: PatchDocument,
    pub provider: &'static str,
    /// `keywords` or `ollama` — how transform coordinates were chosen.
    pub placement: &'static str,
    pub submitted: bool,
}

/// Write PNG + return asset id, upsert_entity op JSON, art provider, placement label.
pub fn generate_art_asset(
    prompt: &str,
    mode: ArtMode,
    placement_mode: ArtPlacementMode,
    entity_id: &str,
    scene_id: &str,
    assets_dir: &Path,
) -> EngineResult<(String, serde_json::Value, &'static str, &'static str)> {
    let asset_id = format!("art_{}", simple_nonce());
    let path = assets_dir.join(format!("{asset_id}.png"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| aether_core::EngineError::InvalidPackage(e.to_string()))?;
    }
    art_backends::write_art_png(&path, prompt, mode)?;
    let (x, y, placement) = art_placement::resolve_placement(prompt, placement_mode);
    let op = create_sprite_op(&asset_id, entity_id, scene_id, x, y);
    let provider = art_provider_label(mode);
    Ok((asset_id, op, provider, placement))
}

pub fn create_sprite_op(
    asset_id: &str,
    entity_id: &str,
    scene_id: &str,
    x: f32,
    y: f32,
) -> serde_json::Value {
    json!({
        "op": "upsert_entity",
        "scene": scene_id,
        "entity": {
            "id": entity_id,
            "scene": scene_id,
            "components": {
                "transform": { "x": x, "y": y },
                "sprite": { "asset": asset_id }
            }
        }
    })
}

fn art_provider_label(mode: ArtMode) -> &'static str {
    match mode {
        ArtMode::Stub => "stub",
        ArtMode::Ollama => "ollama",
        ArtMode::Automatic1111 => "automatic1111",
    }
}

pub fn generate_art(
    prompt: &str,
    mode: ArtMode,
    placement_mode: ArtPlacementMode,
    entity_id: &str,
    scene_id: &str,
    package_version: u64,
    assets_dir: &Path,
) -> EngineResult<(String, PatchDocument, &'static str, &'static str)> {
    let (asset_id, op, provider, placement) =
        generate_art_asset(prompt, mode, placement_mode, entity_id, scene_id, assets_dir)?;
    let patch = PatchDocument {
        patch_id: format!("art_{asset_id}"),
        base_version: package_version,
        ops: vec![op],
    };
    Ok((asset_id, patch, provider, placement))
}

pub(crate) fn simple_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{n:x}")
}
