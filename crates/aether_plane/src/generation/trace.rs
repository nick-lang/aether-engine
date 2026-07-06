//! Observability for LLM patch generation — returned to Studio for debugging.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// What was sent to / returned from the model, plus validation notes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationTrace {
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub user_message: String,
    /// Plan JSON from the model (ollama_plan) or legacy full patch JSON.
    pub raw_model_text: String,
    /// Built patch ops summary for Studio (e.g. "1 room, 1 collectible").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_notes: Vec<String>,
}

impl GenerationTrace {
    pub fn new(provider: &str, model: Option<String>, user_message: String, raw_model_text: String) -> Self {
        Self {
            provider: provider.to_string(),
            model,
            user_message,
            raw_model_text,
            parse_error: None,
            validation_notes: Vec::new(),
            built_summary: None,
        }
    }
}

/// Saved under `data/plane/{game_id}/traces/` after LLM generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedGenerationTrace {
    pub saved_at_ms: u64,
    pub game_id: String,
    pub patch_id: String,
    pub prompt: String,
    pub provider: String,
    pub trace: GenerationTrace,
}

pub fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn sanitize_trace_filename_part(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn trace_file_name(saved_at_ms: u64, patch_id: &str) -> String {
    format!(
        "{saved_at_ms}_{}.json",
        sanitize_trace_filename_part(patch_id)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_filename_sanitizes_patch_id() {
        let name = trace_file_name(1, "patch/foo bar");
        assert_eq!(name, "1_patch_foo_bar.json");
    }
}
