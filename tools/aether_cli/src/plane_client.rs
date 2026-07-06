//! HTTP client for the content plane (agent / CLI automation).

use std::process::{Child, Command};
use std::time::Duration;

use aether_package::GamePackage;
use reqwest::blocking::Client;
use serde_json::Value;

#[derive(Clone)]
pub struct PlaneClient {
    pub base_url: String,
    client: Client,
}

impl PlaneClient {
    pub fn from_env() -> Self {
        let base_url =
            std::env::var("AETHER_PLANE_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into());
        Self::new(base_url.trim_end_matches('/'))
    }

    pub fn new(base_url: impl Into<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(600))
            .build()
            .expect("http client");
        Self {
            base_url: base_url.into(),
            client,
        }
    }

    pub fn health_ok(&self) -> bool {
        self.client
            .get(format!("{}/health", self.base_url))
            .send()
            .ok()
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    pub fn wait_for_health(&self, seconds: u64) -> bool {
        for _ in 0..seconds {
            if self.health_ok() {
                return true;
            }
            std::thread::sleep(Duration::from_secs(1));
        }
        false
    }

    /// Background `cargo run -p aether_plane` from repo root.
    pub fn spawn_plane_process() -> Result<Child, String> {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Command::new("cargo")
            .args(["run", "-p", "aether_plane"])
            .current_dir(cwd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())
    }

    pub fn canonical_package(&self, game_id: &str) -> Result<GamePackage, String> {
        let url = format!("{}/packages/{game_id}/canonical", self.base_url);
        let text = self.get_text(&url)?;
        GamePackage::from_json(&text).map_err(|e| e.to_string())
    }

    pub fn manifest(&self, game_id: &str) -> Result<Value, String> {
        let url = format!("{}/packages/{game_id}/manifest", self.base_url);
        self.get_json(&url)
    }

    pub fn list_candidates(&self, game_id: &str, status: Option<&str>) -> Result<Value, String> {
        let mut url = format!("{}/packages/{game_id}/candidates", self.base_url);
        if let Some(s) = status {
            url.push_str(&format!("?status={s}"));
        }
        self.get_json(&url)
    }

    pub fn generate(
        &self,
        game_id: &str,
        prompt: &str,
        mode: &str,
        auto_submit: bool,
    ) -> Result<Value, String> {
        let url = format!("{}/packages/{game_id}/generate", self.base_url);
        let body = serde_json::json!({
            "prompt": prompt,
            "mode": mode,
            "auto_submit": auto_submit,
        });
        self.post_json(&url, &body)
    }

    pub fn publish(&self, game_id: &str, patch_id: &str) -> Result<Value, String> {
        let url = format!(
            "{}/packages/{game_id}/candidates/{patch_id}/publish",
            self.base_url
        );
        self.post_empty(&url)
    }

    pub fn rollback(&self, game_id: &str, patch_id: Option<&str>) -> Result<Value, String> {
        let url = format!("{}/packages/{game_id}/rollback", self.base_url);
        let body = match patch_id {
            Some(id) => serde_json::json!({ "patch_id": id }),
            None => serde_json::json!({}),
        };
        self.post_json(&url, &body)
    }

    pub fn ollama_activity(&self) -> Result<Value, String> {
        let url = format!("{}/health/ollama/activity", self.base_url);
        self.get_json(&url)
    }

    pub fn list_traces(&self, game_id: &str) -> Result<Value, String> {
        let url = format!("{}/packages/{game_id}/traces", self.base_url);
        self.get_json(&url)
    }

    fn get_json(&self, url: &str) -> Result<Value, String> {
        let text = self.get_text(url)?;
        serde_json::from_str(&text).map_err(|e| format!("invalid json from {url}: {e}"))
    }

    fn get_text(&self, url: &str) -> Result<String, String> {
        let res = self
            .client
            .get(url)
            .send()
            .map_err(|e| connection_err(url, &e))?;
        let status = res.status();
        let text = res.text().map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("GET {url} → {status}: {text}"));
        }
        Ok(text)
    }

    fn post_json(&self, url: &str, body: &Value) -> Result<Value, String> {
        let res = self
            .client
            .post(url)
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .map_err(|e| connection_err(url, &e))?;
        let status = res.status();
        let text = res.text().map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("POST {url} → {status}: {text}"));
        }
        serde_json::from_str(&text).map_err(|e| format!("invalid json: {e}\n{text}"))
    }

    fn post_empty(&self, url: &str) -> Result<Value, String> {
        let res = self
            .client
            .post(url)
            .send()
            .map_err(|e| connection_err(url, &e))?;
        let status = res.status();
        let text = res.text().map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("POST {url} → {status}: {text}"));
        }
        if text.trim().is_empty() {
            return Ok(Value::Null);
        }
        serde_json::from_str(&text).map_err(|e| format!("invalid json: {e}\n{text}"))
    }
}

fn connection_err(url: &str, err: &reqwest::Error) -> String {
    format!(
        "could not reach {url}: {err} (start plane: cargo run -p aether_cli -- plane ensure --spawn)"
    )
}
