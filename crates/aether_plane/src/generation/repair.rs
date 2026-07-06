//! Parse model output into a [`PatchDocument`] (strip fences, etc.).

use aether_core::{EngineError, EngineResult};
use aether_patch::PatchDocument;
use serde_json::Value;

pub fn parse_json_value(raw: &str) -> EngineResult<Value> {
    let trimmed = raw.trim();
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }
    if let Some(inner) = extract_markdown_json(trimmed) {
        return serde_json::from_str(&inner)
            .map_err(|e| EngineError::InvalidPackage(format!("json repair failed: {e}")));
    }
    Err(EngineError::InvalidPackage(
        "could not parse JSON from model output".into(),
    ))
}

pub fn parse_patch_json(raw: &str) -> EngineResult<PatchDocument> {
    let trimmed = raw.trim();
    if let Ok(patch) = serde_json::from_str::<PatchDocument>(trimmed) {
        return Ok(patch);
    }

    if let Some(inner) = extract_markdown_json(trimmed) {
        return serde_json::from_str(&inner)
            .map_err(|e| EngineError::InvalidPackage(format!("json repair failed: {e}")));
    }

    Err(EngineError::InvalidPackage(
        "could not parse patch JSON from model output".into(),
    ))
}

fn extract_markdown_json(text: &str) -> Option<String> {
    let start = text.find("```json").or_else(|| text.find("```"))?;
    let after = &text[start..];
    let content_start = after.find('\n')? + 1;
    let rest = &after[content_start..];
    let end = rest.find("```")?;
    Some(rest[..end].trim().to_string())
}
