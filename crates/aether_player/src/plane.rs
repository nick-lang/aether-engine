//! Blocking SSE client for the Aether content plane.

use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use aether_patch::PatchDocument;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct PlaneConfig {
    pub base_url: String,
    pub game_id: String,
}

#[derive(Clone)]
pub struct PlaneSync {
    pub patches: Arc<Mutex<Vec<PatchDocument>>>,
    pub reload_canonical: Arc<AtomicBool>,
}

#[derive(Debug, Deserialize)]
struct ManifestResponse {
    published_sequence: u64,
}

#[derive(Debug, Deserialize)]
struct StreamEnvelope {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    patch: Option<PatchDocument>,
    #[serde(default)]
    game_id: Option<String>,
}

pub fn spawn_subscriber(config: PlaneConfig, sync: PlaneSync) {
    std::thread::spawn(move || {
        if let Err(err) = run_subscriber(&config, &sync) {
            eprintln!("Plane subscriber ended: {err}");
        }
    });
}

fn run_subscriber(config: &PlaneConfig, sync: &PlaneSync) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .build()
        .map_err(|e| e.to_string())?;

    let base = config.base_url.trim_end_matches('/');
    let manifest_url = format!("{base}/packages/{}/manifest", config.game_id);
    let since = client
        .get(&manifest_url)
        .send()
        .map_err(|e| e.to_string())?
        .json::<ManifestResponse>()
        .map(|m| m.published_sequence)
        .unwrap_or(0);

    let stream_url = format!(
        "{base}/packages/{}/stream?since={since}",
        config.game_id
    );
    println!(
        "Plane: stream {stream_url} (replays publishes with sequence > {since} only)"
    );

    let response = client
        .get(&stream_url)
        .header("Accept", "text/event-stream")
        .send()
        .map_err(|e| e.to_string())?;

    if !response.status().is_success() {
        return Err(format!("stream HTTP {}", response.status()));
    }

    let reader = BufReader::new(response);
    let mut data_lines = Vec::new();

    for line in reader.lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.is_empty() {
            if let Some(patch) = parse_sse_patch(&data_lines) {
                if let Ok(mut queue) = sync.patches.lock() {
                    queue.push(patch);
                }
            } else if parse_sse_canonical_reset(&data_lines, &config.game_id) {
                sync.reload_canonical.store(true, Ordering::Relaxed);
                println!("Plane: canonical reset — reloading world from plane");
            }
            data_lines.clear();
            continue;
        }
        if let Some(payload) = line.strip_prefix("data:") {
            data_lines.push(payload.trim().to_string());
        }
    }

    Ok(())
}

fn parse_sse_patch(data_lines: &[String]) -> Option<PatchDocument> {
    let body = data_lines.join("\n");
    let envelope: StreamEnvelope = serde_json::from_str(&body).ok()?;
    if envelope.event_type == "patch_published" {
        envelope.patch
    } else {
        None
    }
}

fn parse_sse_canonical_reset(data_lines: &[String], expected_game: &str) -> bool {
    let body = data_lines.join("\n");
    let Ok(envelope) = serde_json::from_str::<StreamEnvelope>(&body) else {
        return false;
    };
    envelope.event_type == "canonical_reset"
        && envelope
            .game_id
            .as_deref()
            .is_none_or(|id| id == expected_game)
}

pub fn fetch_canonical(config: &PlaneConfig) -> Result<aether_package::GamePackage, String> {
    let url = format!(
        "{}/packages/{}/canonical",
        config.base_url.trim_end_matches('/'),
        config.game_id
    );
    let text = reqwest::blocking::get(&url)
        .map_err(|e| e.to_string())?
        .text()
        .map_err(|e| e.to_string())?;
    let package = aether_package::GamePackage::from_json(&text).map_err(|e| e.to_string())?;
    aether_package::validate_package(&package).map_err(|e| e.to_string())?;
    Ok(package)
}
