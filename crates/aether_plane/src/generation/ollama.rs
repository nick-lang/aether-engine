//! Ollama local LLM — patch JSON via /api/chat (no API cost).

use aether_core::{EngineError, EngineResult};
use aether_patch::PatchDocument;
use serde::Serialize;

use super::llm_validate::validate_llm_patch;
use super::prompts::{patch_user_message, PATCH_SYSTEM_PROMPT};
use super::repair::parse_patch_json;
use super::trace::GenerationTrace;
use aether_package::GamePackage;

pub fn ollama_url() -> String {
    std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into())
}

pub fn ollama_model() -> String {
    std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "llama3.2".into())
}

/// Default 10 minutes — large local models (e.g. qwen3.6) often exceed 120s for JSON patches.
pub fn ollama_timeout_secs() -> u64 {
    std::env::var("OLLAMA_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&s| s > 0)
        .unwrap_or(180)
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaHealthReport {
    pub ok: bool,
    pub url: String,
    pub configured_model: String,
    pub model_available: bool,
    pub models: Vec<String>,
    pub timeout_secs: u64,
    pub message: String,
}

/// Ping Ollama `/api/tags` and confirm [`OLLAMA_MODEL`](ollama_model) is installed.
pub fn check_ollama_health() -> OllamaHealthReport {
    let url = ollama_url();
    let model = ollama_model();
    let timeout_secs = ollama_timeout_secs();

    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return OllamaHealthReport {
                ok: false,
                url,
                configured_model: model,
                model_available: false,
                models: vec![],
                timeout_secs,
                message: format!("HTTP client error: {e}"),
            };
        }
    };

    let tags_url = format!("{}/api/tags", url.trim_end_matches('/'));
    let response = match client.get(&tags_url).send() {
        Ok(r) => r,
        Err(e) => {
            return OllamaHealthReport {
                ok: false,
                url,
                configured_model: model,
                model_available: false,
                models: vec![],
                timeout_secs,
                message: format!(
                    "Cannot reach Ollama at {tags_url}: {e} (is `ollama serve` running?)"
                ),
            };
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return OllamaHealthReport {
            ok: false,
            url,
            configured_model: model,
            model_available: false,
            models: vec![],
            timeout_secs,
            message: format!("Ollama HTTP {status}: {body}"),
        };
    }

    let body = match response.text() {
        Ok(t) => t,
        Err(e) => {
            return OllamaHealthReport {
                ok: false,
                url,
                configured_model: model,
                model_available: false,
                models: vec![],
                timeout_secs,
                message: format!("Failed to read Ollama response: {e}"),
            };
        }
    };

    let models = parse_tags_models(&body);
    let model_available = models.iter().any(|name| name == &model || model_matches(name, &model));
    let ok = model_available;
    let message = if model_available {
        format!("Ollama OK — model {model:?} is available ({tags_url})")
    } else if models.is_empty() {
        format!("Ollama responded but no models listed; run `ollama pull {model}`")
    } else {
        format!(
            "Ollama OK but {model:?} not found. Installed: {}. Run `ollama pull {model}`.",
            models.join(", ")
        )
    };

    OllamaHealthReport {
        ok,
        url,
        configured_model: model,
        model_available,
        models,
        timeout_secs,
        message,
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OllamaActivityReport {
    /// `idle` | `loading` | `running` | `unreachable`
    pub state: String,
    pub reachable: bool,
    pub configured_model: String,
    pub running_models: Vec<String>,
    pub message: String,
}

/// Live view via Ollama `GET /api/ps` — shows models currently loaded in VRAM.
pub fn ollama_activity() -> OllamaActivityReport {
    let url = ollama_url();
    let model = ollama_model();
    let client = match reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return OllamaActivityReport {
                state: "unreachable".into(),
                reachable: false,
                configured_model: model,
                running_models: vec![],
                message: format!("HTTP client error: {e}"),
            };
        }
    };

    let ps_url = format!("{}/api/ps", url.trim_end_matches('/'));
    let response = match client.get(&ps_url).send() {
        Ok(r) => r,
        Err(e) => {
            return OllamaActivityReport {
                state: "unreachable".into(),
                reachable: false,
                configured_model: model,
                running_models: vec![],
                message: format!("Cannot reach Ollama at {ps_url}: {e}"),
            };
        }
    };

    if !response.status().is_success() {
        return OllamaActivityReport {
            state: "unreachable".into(),
            reachable: false,
            configured_model: model,
            running_models: vec![],
            message: format!("Ollama HTTP {}", response.status()),
        };
    }

    let body = response.text().unwrap_or_default();
    let running_models = parse_ps_models(&body);
    let configured_active = running_models
        .iter()
        .any(|name| model_matches(name, &model));

    let (state, message) = if running_models.is_empty() {
        (
            "idle",
            format!(
                "Ollama reachable — no model loaded in VRAM yet (waiting on {model:?} or plane request)"
            ),
        )
    } else if configured_active {
        (
            "running",
            format!(
                "Ollama processing {model:?} (loaded: {})",
                running_models.join(", ")
            ),
        )
    } else {
        (
            "loading",
            format!(
                "Ollama has models loaded ({}) — configured {model:?} may still be loading",
                running_models.join(", ")
            ),
        )
    };

    OllamaActivityReport {
        state: state.into(),
        reachable: true,
        configured_model: model,
        running_models,
        message: message.to_string(),
    }
}

fn parse_ps_models(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return vec![];
    };
    v["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m["name"]
                        .as_str()
                        .or_else(|| m["model"].as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_tags_models(body: &str) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(body) else {
        return vec![];
    };
    v["models"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|m| {
                    m["name"]
                        .as_str()
                        .or_else(|| m["model"].as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// `qwen3.6` matches `qwen3.6:latest`.
fn model_matches(installed: &str, configured: &str) -> bool {
    installed == configured
        || installed.starts_with(&format!("{configured}:"))
        || configured.starts_with(&format!("{installed}:"))
}

pub fn map_ollama_request_error(err: reqwest::Error) -> EngineError {
    let model = ollama_model();
    let secs = ollama_timeout_secs();
    if err.is_timeout() {
        return EngineError::InvalidPackage(format!(
            "Ollama timed out after {secs}s waiting for {model:?}. \
             Large models need longer — set OLLAMA_TIMEOUT_SECS=900 in .env, or use Placement: Keywords to skip a second Ollama call."
        ));
    }
    EngineError::InvalidPackage(format!("Ollama request failed: {err} (model {model:?})"))
}

pub fn format_ollama_http_error(status: reqwest::StatusCode, body: &str, model: &str) -> String {
    let detail = extract_ollama_error_json(body).unwrap_or_else(|| body.trim().to_string());
    format!(
        "Ollama HTTP {status}: {detail} (OLLAMA_MODEL={model:?}; run `ollama list` and `ollama pull {model}`)"
    )
}

fn extract_ollama_error_json(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("error").and_then(|e| e.as_str()).map(str::to_string)
}

pub struct LlmPatchResult {
    pub patch: PatchDocument,
    pub trace: GenerationTrace,
}

/// Hybrid path: small plan JSON → Rust patch builder (recommended).
pub fn generate_from_prompt(
    prompt: &str,
    package_version: u64,
    scene_id: &str,
    canonical_summary: Option<&str>,
    canonical: &GamePackage,
    pending_patches: &[PatchDocument],
) -> EngineResult<LlmPatchResult> {
    super::ollama_plan::generate_plan_patch(
        prompt,
        package_version,
        scene_id,
        canonical_summary,
        canonical,
        pending_patches,
    )
}

/// Legacy: model emits full PatchDocument (slow; set `OLLAMA_LEGACY_PATCH=1` to use).
pub fn generate_legacy_patch(
    prompt: &str,
    package_version: u64,
    scene_id: &str,
    canonical_summary: Option<&str>,
    canonical: &GamePackage,
) -> EngineResult<LlmPatchResult> {
    let base = ollama_url();
    let model = ollama_model();
    let user = patch_user_message(package_version, scene_id, prompt, canonical_summary);

    let body = serde_json::json!({
        "model": model,
        "stream": false,
        "format": "json",
        "messages": [
            { "role": "system", "content": PATCH_SYSTEM_PROMPT },
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
    let mut trace = GenerationTrace::new("ollama_patch", Some(model.clone()), user, content.clone());

    let mut patch = match parse_patch_json(&content) {
        Ok(p) => p,
        Err(e) => {
            trace.parse_error = Some(e.to_string());
            return Err(e);
        }
    };
    patch.base_version = package_version;
    if patch.patch_id.trim().is_empty() {
        patch.patch_id = format!("ollama_{}", simple_nonce());
    }

    trace.validation_notes = validate_llm_patch(&patch, scene_id, canonical)?;
    Ok(LlmPatchResult { patch, trace })
}

pub fn ollama_use_legacy_patch() -> bool {
    std::env::var("OLLAMA_LEGACY_PATCH")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub(crate) fn extract_message_content(body: &str) -> EngineResult<String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| EngineError::InvalidPackage(format!("Ollama response parse: {e}")))?;
    v["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| EngineError::InvalidPackage("Ollama response missing message.content".into()))
}

pub fn http_client() -> EngineResult<reqwest::blocking::Client> {
    let secs = ollama_timeout_secs();
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(secs))
        .build()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_matches_tag_variants() {
        assert!(model_matches("qwen3.6:latest", "qwen3.6:latest"));
        assert!(model_matches("qwen3.6:latest", "qwen3.6"));
    }
}

fn simple_nonce() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("{n:x}")
}
