//! Local / remote image backends for art generation.

use std::io::Cursor;
use std::path::Path;

use aether_core::{EngineError, EngineResult};
use base64::Engine;
use image::imageops::FilterType;
use image::ImageReader;

use super::prompts::art_prompt_suffix;
use super::ollama::http_client;

use super::ArtMode;

pub fn write_art_png(path: &Path, prompt: &str, mode: ArtMode) -> EngineResult<()> {
    match mode {
        ArtMode::Stub => write_stub_png(path, prompt),
        ArtMode::Ollama => write_ollama_png(path, prompt),
        ArtMode::Automatic1111 => write_a1111_png(path, prompt),
    }
}

fn write_stub_png(path: &Path, prompt: &str) -> EngineResult<()> {
    use image::{ImageBuffer, Rgba};
    let hash = prompt.bytes().fold(0u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let r = ((hash >> 16) & 0xFF) as u8;
    let g = ((hash >> 8) & 0xFF) as u8;
    let b = (hash & 0xFF) as u8;
    let mut img: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(64, 64);
    for (x, y, pixel) in img.enumerate_pixels_mut() {
        let edge = x < 2 || y < 2 || x > 61 || y > 61;
        *pixel = if edge {
            Rgba([20, 24, 32, 255])
        } else {
            Rgba([r, g, b, 255])
        };
    }
    img.save(path)
        .map_err(|e| EngineError::InvalidPackage(format!("write png: {e}")))
}

fn write_ollama_png(path: &Path, prompt: &str) -> EngineResult<()> {
    let base = std::env::var("OLLAMA_URL").unwrap_or_else(|_| "http://127.0.0.1:11434".into());
    let model = std::env::var("OLLAMA_IMAGE_MODEL")
        .or_else(|_| std::env::var("OLLAMA_MODEL"))
        .unwrap_or_else(|_| "x/flux2-klein:4b".into());
    let full_prompt = art_prompt_suffix(prompt);

    let body = serde_json::json!({
        "model": model,
        "prompt": full_prompt,
        "stream": false
    });

    let client = http_client()?;
    let url = format!("{}/api/generate", base.trim_end_matches('/'));
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| EngineError::InvalidPackage(format!("Ollama image request failed: {e}")))?;

    let status = res.status();
    let text = res
        .text()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    if !status.is_success() {
        return Err(EngineError::InvalidPackage(format!(
            "Ollama image HTTP {status}: {text} (try: ollama pull {model}; on Windows use art mode automatic1111 or stub — Ollama image gen is macOS-only today)"
        )));
    }

    let bytes = extract_ollama_image_bytes(&text)?;
    save_png_bytes(path, &bytes)
}

fn extract_ollama_image_bytes(body: &str) -> EngineResult<Vec<u8>> {
    let v: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| EngineError::InvalidPackage(format!("Ollama image JSON: {e}")))?;

    if let Some(images) = v.get("images").and_then(|i| i.as_array()) {
        if let Some(first) = images.first().and_then(|i| i.as_str()) {
            return base64::engine::general_purpose::STANDARD
                .decode(first)
                .map_err(|e| EngineError::InvalidPackage(format!("Ollama image base64: {e}")));
        }
    }

    Err(EngineError::InvalidPackage(
        "Ollama returned no images — use an image model (e.g. ollama pull flux) or art mode automatic1111".into(),
    ))
}

fn write_a1111_png(path: &Path, prompt: &str) -> EngineResult<()> {
    let base = std::env::var("A1111_URL").unwrap_or_else(|_| "http://127.0.0.1:7860".into());
    let full_prompt = art_prompt_suffix(prompt);

    let body = serde_json::json!({
        "prompt": full_prompt,
        "negative_prompt": "blurry, text, watermark, realistic photo",
        "width": 128,
        "height": 128,
        "steps": 20,
        "cfg_scale": 7.0,
        "sampler_name": "Euler a"
    });

    let client = http_client()?;
    let url = format!("{}/sdapi/v1/txt2img", base.trim_end_matches('/'));
    let res = client
        .post(&url)
        .json(&body)
        .send()
        .map_err(|e| {
            EngineError::InvalidPackage(format!(
                "Automatic1111 request failed: {e} (is WebUI running with --api?)"
            ))
        })?;

    let status = res.status();
    let text = res
        .text()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?;
    if !status.is_success() {
        let detail = summarize_a1111_error_body(&text);
        return Err(EngineError::InvalidPackage(format!(
            "Automatic1111 HTTP {status}: {detail}"
        )));
    }

    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| EngineError::InvalidPackage(format!("A1111 response: {e}")))?;
    let b64 = v["images"]
        .get(0)
        .and_then(|i| i.as_str())
        .ok_or_else(|| EngineError::InvalidPackage("A1111 returned no images".into()))?;

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| EngineError::InvalidPackage(format!("A1111 base64: {e}")))?;
    save_png_bytes(path, &bytes)
}

fn summarize_a1111_error_body(text: &str) -> String {
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(text) {
        if let Some(arr) = v.get("errors").and_then(|x| x.as_array()) {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|e| e.as_str().map(str::to_string))
                .collect();
            if !parts.is_empty() {
                return parts.join("; ");
            }
        }
        for key in ["error", "detail", "message"] {
            if let Some(msg) = v.get(key).and_then(|x| x.as_str()) {
                return msg.to_string();
            }
        }
    }
    let trimmed = text.trim();
    if trimmed.len() > 800 {
        format!("{}…", &trimmed[..800])
    } else {
        trimmed.to_string()
    }
}

fn save_png_bytes(path: &Path, bytes: &[u8]) -> EngineResult<()> {
    let img = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| EngineError::InvalidPackage(e.to_string()))?
        .decode()
        .map_err(|e| EngineError::InvalidPackage(format!("decode image: {e}")))?;

    let resized = img.resize_exact(64, 64, FilterType::Nearest);
    resized
        .save(path)
        .map_err(|e| EngineError::InvalidPackage(format!("write png: {e}")))
}
