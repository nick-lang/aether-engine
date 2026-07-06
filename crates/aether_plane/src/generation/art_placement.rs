//! Resolve sprite `transform` for art patches — keyword heuristics or Ollama JSON.

use aether_core::{EngineError, EngineResult};
use serde::Deserialize;

use super::ollama::{
    self, format_ollama_http_error, map_ollama_request_error, ollama_model, ollama_url,
};

const PLACEMENT_SYSTEM: &str = r#"You place a 2D game prop in a top-down scene. Output ONLY valid JSON:
{"x": number, "y": number}

Rules:
- Origin (0, 0) is the scene center; keep |x| and |y| at most 200 unless the prompt clearly needs farther.
- Cardinal directions: north → positive y, south → negative y, east → positive x, west → negative x.
- "near the player" or "center" → small coordinates near 0.
- No markdown, no explanation."#;

#[derive(Debug, Clone, Copy, Default, Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtPlacementMode {
    /// Try Ollama when `OLLAMA_URL` is set, else keywords.
    #[default]
    Auto,
    Keywords,
    Ollama,
}

/// Returns `(x, y, provider label)`.
pub fn resolve_placement(prompt: &str, mode: ArtPlacementMode) -> (f32, f32, &'static str) {
    match mode {
        ArtPlacementMode::Keywords => {
            let (x, y) = placement_from_keywords(prompt);
            (x, y, "keywords")
        }
        ArtPlacementMode::Ollama => match placement_from_ollama(prompt) {
            Ok((x, y)) => (x, y, "ollama"),
            Err(_) => {
                let (x, y) = placement_from_keywords(prompt);
                (x, y, "keywords")
            }
        },
        ArtPlacementMode::Auto => {
            if ollama_configured() {
                if let Ok((x, y)) = placement_from_ollama(prompt) {
                    return (x, y, "ollama");
                }
            }
            let (x, y) = placement_from_keywords(prompt);
            (x, y, "keywords")
        }
    }
}

fn ollama_configured() -> bool {
    std::env::var("OLLAMA_URL").is_ok() || std::env::var("OLLAMA_MODEL").is_ok()
}

pub fn placement_from_keywords(prompt: &str) -> (f32, f32) {
    use aether_package::{
        attach_room, default_attach_height, default_attach_width, default_main_room_bounds,
        place_in_room, Direction, RoomAnchor,
    };

    let main = default_main_room_bounds();
    let lower = prompt.to_lowercase();
    if let Some(dir) = ["north", "south", "east", "west"]
        .into_iter()
        .find(|d| lower.contains(d))
    {
        if let Some(d) = Direction::parse(dir) {
            let room = attach_room(
                &main,
                d,
                default_attach_width(d),
                default_attach_height(d),
            );
            return place_in_room(&room, RoomAnchor::for_direction(d));
        }
    }
    if lower.contains("center") || lower.contains("middle") {
        return place_in_room(&main, RoomAnchor::Center);
    }
    place_in_room(&main, RoomAnchor::East)
}

fn placement_from_ollama(prompt: &str) -> EngineResult<(f32, f32)> {
    let base = ollama_url();
    let model = ollama_model();

    let body = serde_json::json!({
        "model": model,
        "stream": false,
        "format": "json",
        "messages": [
            { "role": "system", "content": PLACEMENT_SYSTEM },
            { "role": "user", "content": prompt }
        ]
    });

    let client = ollama::http_client()?;
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
    parse_placement_json(&content)
}

fn extract_message_content(body: &str) -> EngineResult<String> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| EngineError::InvalidPackage(format!("Ollama placement parse: {e}")))?;
    v["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| EngineError::InvalidPackage("Ollama placement missing content".into()))
}

#[derive(Debug, Deserialize)]
struct PlacementCoords {
    x: f64,
    y: f64,
}

fn parse_placement_json(raw: &str) -> EngineResult<(f32, f32)> {
    let trimmed = raw.trim();
    if let Ok(coords) = serde_json::from_str::<PlacementCoords>(trimmed) {
        return Ok(clamp_placement(coords.x, coords.y));
    }
    if let Some(inner) = extract_markdown_json(trimmed) {
        let coords: PlacementCoords = serde_json::from_str(&inner)
            .map_err(|e| EngineError::InvalidPackage(format!("placement json: {e}")))?;
        return Ok(clamp_placement(coords.x, coords.y));
    }
    Err(EngineError::InvalidPackage(
        "could not parse placement JSON from model".into(),
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

fn clamp_placement(x: f64, y: f64) -> (f32, f32) {
    const LIMIT: f64 = 250.0;
    let x = x.clamp(-LIMIT, LIMIT) as f32;
    let y = y.clamp(-LIMIT, LIMIT) as f32;
    (x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keywords_east() {
        let (x, y) = placement_from_keywords("rustic chest to the east");
        // East wing attached to default main room, prop on inner east edge.
        assert_eq!((x, y), (308.0, 0.0));
    }

    #[test]
    fn parse_placement_object() {
        let (x, y) = parse_placement_json(r#"{"x": 80, "y": -30}"#).unwrap();
        assert!((x - 80.0).abs() < f32::EPSILON);
        assert!((y + 30.0).abs() < f32::EPSILON);
    }
}
