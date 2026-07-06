//! OpenAI chat completions → patch JSON.

use aether_core::{EngineError, EngineResult};

use super::llm_validate::validate_llm_patch;
use super::ollama::LlmPatchResult;
use super::prompts::{patch_user_message, PATCH_SYSTEM_PROMPT};
use super::repair::parse_patch_json;
use super::trace::GenerationTrace;
use aether_package::GamePackage;

pub fn generate_from_prompt(
    prompt: &str,
    package_version: u64,
    scene_id: &str,
    canonical_summary: Option<&str>,
    canonical: &GamePackage,
) -> EngineResult<LlmPatchResult> {
    let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| {
        EngineError::InvalidPackage(
            "OPENAI_API_KEY is not set — use simulated mode or add key to .env".into(),
        )
    })?;

    let model = std::env::var("OPENAI_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());
    let user = patch_user_message(package_version, scene_id, prompt, canonical_summary);

    let body = serde_json::json!({
        "model": model,
        "temperature": 0.2,
        "messages": [
            { "role": "system", "content": PATCH_SYSTEM_PROMPT },
            { "role": "user", "content": user }
        ]
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;

    let res = client
        .post("https://api.openai.com/v1/chat/completions")
        .bearer_auth(api_key)
        .json(&body)
        .send()
        .map_err(|e| EngineError::InvalidPackage(format!("OpenAI request failed: {e}")))?;

    let status = res.status();
    let text = res
        .text()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    if !status.is_success() {
        return Err(EngineError::InvalidPackage(format!(
            "OpenAI HTTP {status}: {text}"
        )));
    }

    let content = extract_assistant_content(&text)?;
    let mut trace = GenerationTrace::new("openai", Some(model.clone()), user, content.clone());

    let mut patch = match parse_patch_json(&content) {
        Ok(p) => p,
        Err(e) => {
            trace.parse_error = Some(e.to_string());
            return Err(e);
        }
    };
    patch.base_version = package_version;
    if patch.patch_id.trim().is_empty() {
        patch.patch_id = format!("openai_{}", simple_nonce());
    }

    trace.validation_notes = validate_llm_patch(&patch, scene_id, canonical)?;
    Ok(LlmPatchResult { patch, trace })
}

fn extract_assistant_content(body: &str) -> EngineResult<String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| EngineError::InvalidPackage(format!("OpenAI response parse: {e}")))?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| EngineError::InvalidPackage("OpenAI response missing content".into()))
}

fn simple_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{n:x}")
}
