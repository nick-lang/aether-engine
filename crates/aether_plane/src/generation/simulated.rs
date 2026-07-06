//! Deterministic “LLM” for local dev — maps prompts to valid patches.

use aether_core::{EngineError, EngineResult};
use aether_package::GamePackage;
use aether_patch::PatchDocument;

use super::game_builder::{build_package, new_game_patch_id, package_to_audit_patch, validate_built_package};
use super::game_plan::{parse_game_prompt, prompt_is_full_game};
use super::incremental_plan::parse_plan_from_keywords;
use super::plan_builder::plan_to_patch;

pub fn generate_from_prompt(
    prompt: &str,
    package_version: u64,
    scene_id: &str,
    game_id: &str,
    canonical: &GamePackage,
    pending_patches: &[PatchDocument],
) -> EngineResult<PatchDocument> {
    if prompt_is_full_game(prompt) {
        let plan = parse_game_prompt(prompt);
        let package = build_package(game_id, &plan);
        validate_built_package(&package)?;
        let mut patch = package_to_audit_patch(&package, &new_game_patch_id());
        patch.base_version = package_version;
        return Ok(patch);
    }

    if let Some(plan) = parse_plan_from_keywords(prompt) {
        return plan_to_patch(&plan, package_version, scene_id, canonical, pending_patches);
    }

    Err(EngineError::InvalidPackage(
        "simulated generator: try a prompt about a room (e.g. 'add a room to the north') or a shard/collectible".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn northern_shard_prompt() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = aether_package::load_package(path).unwrap();
        let patch = generate_from_prompt(
            "add a shard to the north",
            1,
            "main",
            "minimal_explorer",
            &canonical,
            &[],
        )
        .unwrap();
        assert_eq!(patch.base_version, 1);
        assert_eq!(patch.upsert_entity_ops().count(), 1);
    }

    #[test]
    fn northern_room_prompt() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let canonical = aether_package::load_package(path).unwrap();
        let patch = generate_from_prompt(
            "add a room to the north",
            1,
            "main",
            "minimal_explorer",
            &canonical,
            &[],
        )
        .unwrap();
        assert_eq!(patch.base_version, 1);
        assert_eq!(patch.upsert_room_ops().count(), 1);
        assert_eq!(patch.paint_tiles_ops().count(), 1);
    }
}
