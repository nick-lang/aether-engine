//! `aether plane …` subcommands for agents and scripts.

use std::process::{exit, Command, Stdio};
use std::thread;
use std::time::Duration;

use crate::plane_client::PlaneClient;
use crate::playtest::{playtest_package, print_report};

pub fn run_plane_subcommand(args: &[String]) {
    let sub = args.first().map(String::as_str);
    match sub {
        Some("seed") => {
            let game_id = args.get(1).expect("game_id");
            let path = args.get(2).expect("package.json");
            run_plane_binary(&["seed", game_id, path]);
        }
        Some("reset") => {
            let game_id = args.get(1).expect("game_id");
            run_plane_binary(&["reset", game_id]);
        }
        Some("ensure") => {
            let spawn = args.iter().any(|a| a == "--spawn");
            let wait = flag_u64(args, "--wait").unwrap_or(45);
            if !run_ensure(spawn, wait) {
                exit(1);
            }
        }
        Some("health") => {
            let client = PlaneClient::from_env();
            if client.health_ok() {
                println!("ok {}", client.base_url);
            } else {
                eprintln!("down {}", client.base_url);
                exit(1);
            }
        }
        Some("ollama") => {
            let client = PlaneClient::from_env();
            let v = client.ollama_activity().unwrap_or_else(|e| fail(&e));
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Some("canonical") => {
            let game_id = args.get(1).expect("game_id");
            let json = args.iter().any(|a| a == "--json");
            run_canonical(game_id, json);
        }
        Some("manifest") => {
            let game_id = args.get(1).expect("game_id");
            let client = PlaneClient::from_env();
            let v = client.manifest(game_id).unwrap_or_else(|e| fail(&e));
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Some("candidates") => {
            let game_id = args.get(1).expect("game_id");
            let status = flag_str(args, "--status");
            let client = PlaneClient::from_env();
            let v = client
                .list_candidates(game_id, status.as_deref())
                .unwrap_or_else(|e| fail(&e));
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        Some("playtest") => {
            let game_id = args.get(1).expect("game_id");
            let json = args.iter().any(|a| a == "--json");
            if !run_playtest(game_id, json) {
                exit(1);
            }
        }
        Some("smoke") => {
            let game_id = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| "minimal_explorer".into());
            let mode = flag_str(args, "--mode").unwrap_or_else(|| "simulated".into());
            if !run_smoke(&game_id, &mode) {
                exit(1);
            }
        }
        Some("rollback") => {
            let game_id = args.get(1).expect("game_id");
            let patch_id = args.get(2);
            let client = PlaneClient::from_env();
            let v = client
                .rollback(game_id, patch_id.map(String::as_str))
                .unwrap_or_else(|e| fail(&e));
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()));
        }
        Some("submit") => {
            let game_id = args.get(1).expect("game_id");
            let patch_path = args.get(2).expect("patch.json");
            plane_http_post_file(game_id, "candidates", patch_path);
        }
        Some("publish") => {
            let game_id = args.get(1).expect("game_id");
            let patch_id = args.get(2).expect("patch_id");
            let client = PlaneClient::from_env();
            let v = client.publish(game_id, patch_id).unwrap_or_else(|e| fail(&e));
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string()));
        }
        Some("generate") => {
            let game_id = args.get(1).expect("game_id");
            let json = args.iter().any(|a| a == "--json");
            let mode = flag_str(args, "--mode").unwrap_or_else(|| "simulated".into());
            let no_submit = args.iter().any(|a| a == "--no-submit");
            let prompt = args
                .iter()
                .skip(2)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            if prompt.is_empty() {
                eprintln!("usage: aether plane generate <game_id> <prompt> [--mode dream|ollama|simulated]");
                exit(1);
            }
            run_generate(game_id, &prompt, &mode, !no_submit, json);
        }
        Some("traces") => {
            let game_id = args.get(1).expect("game_id");
            let json = args.iter().any(|a| a == "--json");
            let client = PlaneClient::from_env();
            let v = client
                .list_traces(game_id)
                .unwrap_or_else(|e| fail(&e));
            if json {
                println!("{}", serde_json::to_string_pretty(&v).unwrap());
            } else {
                let traces = v
                    .get("traces")
                    .and_then(|t| t.as_array())
                    .cloned()
                    .unwrap_or_default();
                if traces.is_empty() {
                    println!("no traces for {game_id}");
                } else {
                    for t in traces {
                        let patch = t.get("patch_id").and_then(|p| p.as_str()).unwrap_or("?");
                        let provider = t.get("provider").and_then(|p| p.as_str()).unwrap_or("?");
                        let prompt = t.get("prompt").and_then(|p| p.as_str()).unwrap_or("");
                        let ms = t
                            .get("saved_at_ms")
                            .and_then(|m| m.as_u64())
                            .unwrap_or(0);
                        println!("{ms} {provider} patch={patch} prompt={prompt:?}");
                    }
                }
            }
        }
        Some("--port") => {
            if let Some(port) = args.get(1) {
                std::env::set_var("AETHER_PLANE_PORT", port);
            }
            run_plane_server();
        }
        None => run_plane_server(),
        Some(other) => {
            eprintln!("unknown plane subcommand: {other}");
            print_plane_usage();
            exit(1);
        }
    }
}

pub fn print_plane_usage() {
    eprintln!(
        r#"plane subcommands:
  ensure [--spawn] [--wait SECS]   wait for /health (optionally start plane)
  health                           liveness check
  ollama                           Ollama activity JSON from plane
  playtest <game_id> [--json]      headless canonical validation
  smoke <game_id> [--mode MODE]    playtest → generate → playtest
  generate <game_id> <prompt> [--mode dream|ollama|simulated] [--json] [--no-submit]
  traces <game_id> [--json]             list persisted LLM generation traces
  canonical <game_id> [--json]
  manifest <game_id>
  candidates <game_id> [--status pending|published|rejected]
  publish <game_id> <patch_id>
  rollback <game_id> [patch_id]
  submit <game_id> <patch.json>
  seed <game_id> <package.json>
  reset <game_id>"#
    );
}

pub fn run_ensure(spawn: bool, wait_secs: u64) -> bool {
    let client = PlaneClient::from_env();
    if client.health_ok() {
        println!("plane up: {}", client.base_url);
        return true;
    }
    if spawn {
        println!("starting content plane in background…");
        let _child = PlaneClient::spawn_plane_process().unwrap_or_else(|e| {
            fail(&e);
        });
        thread::sleep(Duration::from_secs(2));
    } else {
        eprintln!("plane down: {}", client.base_url);
        eprintln!("run: cargo run -p aether_cli -- plane ensure --spawn");
        return false;
    }
    if client.wait_for_health(wait_secs) {
        println!("plane up: {}", client.base_url);
        true
    } else {
        eprintln!("timed out after {wait_secs}s waiting for {}", client.base_url);
        false
    }
}

fn run_canonical(game_id: &str, json: bool) {
    let client = PlaneClient::from_env();
    let package = client
        .canonical_package(game_id)
        .unwrap_or_else(|e| fail(&e));
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&package).expect("serialize package")
        );
    } else {
        println!(
            "{} v{} — {} rooms, {} entities, {} tile layers",
            package.meta.id,
            package.meta.version,
            package.rooms.len(),
            package.entities.len(),
            package.tile_layers.len()
        );
    }
}

fn run_playtest(game_id: &str, json: bool) -> bool {
    let client = PlaneClient::from_env();
    let package = client
        .canonical_package(game_id)
        .unwrap_or_else(|e| fail(&e));
    let report = playtest_package(&package);
    print_report(&report, json);
    report.ok
}

fn run_generate(game_id: &str, prompt: &str, mode: &str, auto_submit: bool, json: bool) {
    let client = PlaneClient::from_env();
    let v = client
        .generate(game_id, prompt, mode, auto_submit)
        .unwrap_or_else(|e| fail(&e));
    if json {
        println!("{}", serde_json::to_string_pretty(&v).unwrap());
    } else {
        let patch_id = v
            .get("patch")
            .and_then(|p| p.get("patch_id"))
            .and_then(|id| id.as_str())
            .unwrap_or("?");
        let provider = v.get("provider").and_then(|p| p.as_str()).unwrap_or("?");
        println!("generate ok: patch_id={patch_id} provider={provider} submitted={auto_submit}");
        if let Some(trace) = v.get("trace") {
            if let Some(err) = trace.get("parse_error").and_then(|e| e.as_str()) {
                println!("  trace parse_error: {err}");
            }
            if let Some(notes) = trace.get("validation_notes").and_then(|n| n.as_array()) {
                for note in notes {
                    if let Some(s) = note.as_str() {
                        println!("  trace: {s}");
                    }
                }
            }
        }
    }
}

/// Quick regression: playtest → generate → playtest (simulated by default).
pub fn run_smoke(game_id: &str, mode: &str) -> bool {
    if !run_ensure(true, 45) {
        return false;
    }
    println!("--- smoke: initial playtest ---");
    if !run_playtest(game_id, false) {
        eprintln!("initial playtest failed (fix seed/canonical first)");
        return false;
    }
    println!("--- smoke: generate ({mode}) ---");
    let client = PlaneClient::from_env();
    let prompt = "add a golden shard to the east";
    match client.generate(game_id, prompt, mode, true) {
        Ok(v) => {
            let patch_id = v
                .get("patch")
                .and_then(|p| p.get("patch_id"))
                .and_then(|id| id.as_str())
                .unwrap_or("?");
            println!("smoke published patch: {patch_id}");
        }
        Err(e) => {
            eprintln!("smoke generate failed: {e}");
            return false;
        }
    }
    println!("--- smoke: post-generate playtest ---");
    run_playtest(game_id, false)
}

fn run_plane_binary(args: &[&str]) {
    let status = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("aether_plane")
        .arg("--")
        .args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("cargo");
    exit(status.code().unwrap_or(1));
}

fn run_plane_server() {
    let status = Command::new("cargo")
        .args(["run", "-p", "aether_plane"])
        .status()
        .expect("cargo");
    exit(status.code().unwrap_or(1));
}

fn plane_http_post_file(_game_id: &str, _path_seg: &str, file: &String) {
    let base = std::env::var("AETHER_PLANE_URL").unwrap_or_else(|_| "http://127.0.0.1:8787".into());
    let game_id = _game_id;
    let url = format!("{base}/packages/{game_id}/{_path_seg}");
    let text = std::fs::read_to_string(file).expect("read patch file");
    let client = reqwest::blocking::Client::new();
    let res = client
        .post(&url)
        .header("Content-Type", "application/json")
        .body(text)
        .send()
        .expect("post");
    println!("{} {}", res.status(), res.text().unwrap_or_default());
}

fn flag_str(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1).cloned())
}

fn flag_u64(args: &[String], name: &str) -> Option<u64> {
    flag_str(args, name)?.parse().ok()
}

fn fail(msg: &str) -> ! {
    eprintln!("{msg}");
    exit(1);
}
