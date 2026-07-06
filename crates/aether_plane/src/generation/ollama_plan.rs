//! Ollama → small plan JSON → Rust [`plan_builder`].

use aether_core::EngineResult;
use aether_package::GamePackage;
use aether_patch::PatchDocument;

use super::incremental_plan::{parse_plan_from_keywords, parse_plan_json, validate_plan, IncrementalPlan};
use super::llm_validate::validate_llm_patch;
use super::ollama::{
    extract_message_content, format_ollama_http_error, http_client, map_ollama_request_error,
    ollama_model, ollama_url, LlmPatchResult,
};
use super::plan_builder::plan_to_patch;
use super::prompts::{plan_user_message, PLAN_SYSTEM_PROMPT};
use super::trace::GenerationTrace;

pub fn generate_plan_patch(
    prompt: &str,
    package_version: u64,
    scene_id: &str,
    canonical_summary: Option<&str>,
    canonical: &GamePackage,
    pending_patches: &[PatchDocument],
) -> EngineResult<LlmPatchResult> {
    let base = ollama_url();
    let model = ollama_model();
    let user = plan_user_message(package_version, scene_id, prompt, canonical_summary);

    let body = serde_json::json!({
        "model": model,
        "stream": false,
        "format": "json",
        "messages": [
            { "role": "system", "content": PLAN_SYSTEM_PROMPT },
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
        .map_err(|e| aether_core::EngineError::InvalidPackage(e.to_string()))?;
    if !status.is_success() {
        return Err(aether_core::EngineError::InvalidPackage(format_ollama_http_error(
            status, &text, &model,
        )));
    }

    let content = extract_message_content(&text)?;
    let mut trace = GenerationTrace::new("ollama_plan", Some(model), user, content.clone());

    let (plan, used_fallback) = resolve_plan(&content, prompt, &mut trace)?;
    validate_plan(&plan)?;

    let patch = plan_to_patch(&plan, package_version, scene_id, canonical, pending_patches)?;
    if used_fallback {
        trace
            .validation_notes
            .push("used keyword fallback plan".into());
    } else {
        trace.validation_notes.push(format!(
            "built patch from model plan ({} action(s))",
            plan.actions.len()
        ));
    }
    trace
        .validation_notes
        .extend(validate_llm_patch(&patch, scene_id, canonical)?);
    trace.built_summary = Some(summarize_patch(&patch));

    Ok(LlmPatchResult { patch, trace })
}

fn summarize_patch(patch: &aether_patch::PatchDocument) -> String {
    let rooms = patch.upsert_room_ops().count();
    let entities = patch.upsert_entity_ops().count();
    let paints = patch.paint_tiles_ops().count();
    format!("{rooms} room(s), {entities} entity op(s), {paints} paint op(s)")
}

fn resolve_plan(
    content: &str,
    prompt: &str,
    trace: &mut GenerationTrace,
) -> EngineResult<(IncrementalPlan, bool)> {
    match parse_plan_json(content) {
        Ok(plan) => Ok((plan, false)),
        Err(e) => {
            trace.parse_error = Some(e.to_string());
            parse_plan_from_keywords(prompt)
                .ok_or(e)
                .map(|plan| (plan, true))
        }
    }
}
