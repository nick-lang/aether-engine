//! Generate a complete playable [`GamePackage`] from a creator prompt.

use std::path::Path;

use aether_core::EngineResult;
use aether_package::GamePackage;
use aether_patch::PatchDocument;
use serde::{Deserialize, Serialize};

use super::art::generate_art_asset;
use super::game_builder::{
    attach_sprite_to_primary_collectible, build_package, new_game_patch_id, package_to_audit_patch,
    validate_built_package,
};
use super::game_plan::parse_game_prompt;
use super::{ArtMode, ArtPlacementMode, GenerateMode};

#[derive(Debug, Deserialize)]
pub struct GenerateGameRequest {
    pub prompt: String,
    /// `simulated` builds deterministically; `ollama` is not used for full games yet.
    #[serde(default)]
    pub patch_mode: GenerateMode,
    #[serde(default)]
    pub art_mode: ArtMode,
    #[serde(default)]
    pub placement: ArtPlacementMode,
    /// Generate a hero sprite on the main collectible when true or omitted.
    #[serde(default)]
    pub include_art: Option<bool>,
    /// Write directly to canonical (recommended). When false, returns package only.
    #[serde(default = "super::default_true")]
    pub apply_canonical: bool,
}

#[derive(Debug, Serialize)]
pub struct GenerateGameResponse {
    pub package: GamePackage,
    pub plan_summary: GamePlanSummary,
    pub patch: PatchDocument,
    pub provider: &'static str,
    pub asset_id: Option<String>,
    pub art_provider: Option<&'static str>,
    pub placement: Option<&'static str>,
    pub art_included: bool,
    pub applied: bool,
}

#[derive(Debug, Serialize)]
pub struct GamePlanSummary {
    pub title: String,
    pub collectible_count: usize,
    pub room_count: usize,
    pub win_collectible_id: String,
}

pub fn generate_game(
    game_id: &str,
    prompt: &str,
    patch_mode: GenerateMode,
    art_mode: ArtMode,
    placement_mode: ArtPlacementMode,
    include_art: Option<bool>,
    assets_dir: &Path,
) -> EngineResult<GenerateGameResponse> {
    if !matches!(patch_mode, GenerateMode::Simulated) {
        // Full-game generation must be reliable; LLM patch mode is for incremental edits.
        tracing::warn!(
            "generate-game: patch_mode {:?} ignored — using simulated planner",
            patch_mode
        );
    }

    let plan = parse_game_prompt(prompt);
    let mut package = build_package(game_id, &plan);
    validate_built_package(&package)?;

    let should_art = include_art.unwrap_or(plan.wants_hero_art);
    let mut asset_id = None;
    let mut art_provider = None;
    let mut placement = None;

    if should_art {
        let entity_id = format!("pickup_{}", plan.win_collectible_id);
        let (aid, _op, provider, place) = generate_art_asset(
            prompt,
            art_mode,
            placement_mode,
            &entity_id,
            &plan.scene_id,
            assets_dir,
        )?;
        attach_sprite_to_primary_collectible(&mut package, &aid, &plan.win_collectible_id);
        validate_built_package(&package)?;
        asset_id = Some(aid);
        art_provider = Some(provider);
        placement = Some(place);
    }

    let patch_id = new_game_patch_id();
    let patch = package_to_audit_patch(&package, &patch_id);
    let plan_summary = GamePlanSummary {
        title: plan.title.clone(),
        collectible_count: plan.collectibles.len(),
        room_count: package.rooms.len(),
        win_collectible_id: plan.win_collectible_id.clone(),
    };

    Ok(GenerateGameResponse {
        package,
        plan_summary,
        patch,
        provider: "simulated",
        asset_id,
        art_provider,
        placement,
        art_included: should_art,
        applied: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn generates_treasure_hunt() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../data/plane/_test_assets");
        let resp = generate_game(
            "minimal_explorer",
            "Generate a treasure hunt game — collect the golden shard in the north",
            GenerateMode::Simulated,
            ArtMode::Stub,
            ArtPlacementMode::Keywords,
            Some(false),
            &dir,
        )
        .expect("generate");
        assert!(resp.package.entities.len() >= 3);
        assert_eq!(resp.provider, "simulated");
    }
}
