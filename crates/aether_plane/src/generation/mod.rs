//! AI patch generation (Phase 3) — simulated provider by default.

mod art;
mod art_backends;
mod art_placement;
mod dream;
mod incremental_plan;
#[cfg(test)]
mod incremental_checklist;
mod ollama;
mod ollama_plan;
mod openai;
mod plan_builder;
mod spatial;
pub mod prompts;
mod llm_validate;
mod repair;
mod rules;
mod trace;
mod game;
mod game_builder;
pub mod game_plan;
mod simulated;
mod world;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtMode {
    #[default]
    Stub,
    Ollama,
    Automatic1111,
}

pub use art::{generate_art, GenerateArtRequest, GenerateArtResponse};
pub use art_placement::ArtPlacementMode;
pub use ollama::{
    check_ollama_health, ollama_activity, ollama_timeout_secs, ollama_use_legacy_patch,
    OllamaActivityReport, OllamaHealthReport,
};
pub use game::{generate_game, GenerateGameRequest, GenerateGameResponse};
pub use game_plan::prompt_is_full_game;
pub use world::{generate_world, GenerateWorldRequest, GenerateWorldResponse};
// HTTP handlers: `generate_art_handler`, `generate_world_handler` in api.rs.

use aether_core::EngineResult;
use aether_package::GamePackage;
use aether_patch::PatchDocument;
use serde::{Deserialize, Serialize};

pub use llm_validate::validate_llm_patch;
pub use ollama::LlmPatchResult;
pub use rules::validate_patch_rules;
pub use trace::{GenerationTrace, PersistedGenerationTrace, trace_file_name, unix_ms_now};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerateMode {
    #[default]
    Simulated,
    /// Local Ollama — http://127.0.0.1:11434 (see OLLAMA_MODEL).
    Ollama,
    /// Ollama authors a full creative patch (no keyword fallback / plan builder).
    Dream,
    OpenAi,
}

#[derive(Debug, Deserialize)]
pub struct GenerateRequest {
    pub prompt: String,
    #[serde(default)]
    pub mode: GenerateMode,
    /// Queue as a pending candidate when true (default).
    #[serde(default = "default_true")]
    pub auto_submit: bool,
}

pub(crate) fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize)]
pub struct GenerateResponse {
    pub patch: PatchDocument,
    pub provider: &'static str,
    pub submitted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<GenerationTrace>,
}

#[derive(Debug)]
pub struct GeneratePatchOutcome {
    pub patch: PatchDocument,
    pub trace: Option<GenerationTrace>,
}

pub fn generate_patch(
    prompt: &str,
    mode: GenerateMode,
    package_version: u64,
    scene_id: &str,
    game_id: &str,
    canonical_summary: Option<&str>,
    canonical: &GamePackage,
    pending_patches: &[PatchDocument],
) -> EngineResult<GeneratePatchOutcome> {
    let (patch, trace) = match mode {
        GenerateMode::Simulated => {
            let patch = simulated::generate_from_prompt(
                prompt,
                package_version,
                scene_id,
                game_id,
                canonical,
                pending_patches,
            )?;
            (patch, None)
        }
        GenerateMode::Ollama => {
            let result = if ollama::ollama_use_legacy_patch() {
                ollama::generate_legacy_patch(
                    prompt,
                    package_version,
                    scene_id,
                    canonical_summary,
                    canonical,
                )?
            } else {
                ollama::generate_from_prompt(
                    prompt,
                    package_version,
                    scene_id,
                    canonical_summary,
                    canonical,
                    pending_patches,
                )?
            };
            (result.patch, Some(result.trace))
        }
        GenerateMode::Dream => {
            let result = dream::generate_dream_patch(
                prompt,
                package_version,
                scene_id,
                canonical_summary,
                canonical,
            )?;
            (result.patch, Some(result.trace))
        }
        GenerateMode::OpenAi => {
            let result = openai::generate_from_prompt(
                prompt,
                package_version,
                scene_id,
                canonical_summary,
                canonical,
            )?;
            (result.patch, Some(result.trace))
        }
    };
    validate_patch_rules(&patch)?;
    Ok(GeneratePatchOutcome { patch, trace })
}

pub fn parse_model_output(raw: &str, package_version: u64) -> EngineResult<PatchDocument> {
    let patch: PatchDocument = repair::parse_patch_json(raw)?;
    patch.validate_version(package_version)?;
    validate_patch_rules(&patch)?;
    Ok(patch)
}
