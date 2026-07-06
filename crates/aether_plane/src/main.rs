//! Aether content plane server.

use std::net::SocketAddr;
use std::path::PathBuf;

use aether_plane::generation::ollama_timeout_secs;
use aether_plane::{router, AppState, PlaneStore, StreamEvent};
use tokio::sync::broadcast;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    load_dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("aether_plane=info".parse()?))
        .init();

    log_generation_env();

    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 2 && args[1] == "seed" {
        return run_seed(&args[2..]);
    }
    if args.len() >= 2 && args[1] == "reset" {
        return run_reset(&args[2..]);
    }

    let data_dir = env_path("AETHER_PLANE_DATA").unwrap_or_else(|| PathBuf::from("data/plane"));
    let port: u16 = std::env::var("AETHER_PLANE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8787);

    let store = PlaneStore::new(&data_dir);
    let (bus, _) = broadcast::channel::<StreamEvent>(256);
    let studio_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../studio");

    let state = AppState {
        store,
        bus,
    };

    let app = router(state, studio_dir);
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Aether content plane listening on http://{addr}");
    tracing::info!("Studio UI: http://{addr}/studio/");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        tracing::info!("Shutting down content plane (Ctrl+C)");
    }
}

fn run_reset(args: &[String]) -> anyhow::Result<()> {
    let game_id = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: aether_plane reset <game_id>"))?;
    let data_dir = env_path("AETHER_PLANE_DATA").unwrap_or_else(|| PathBuf::from("data/plane"));
    let store = PlaneStore::new(&data_dir);
    store.reset_game(game_id)?;
    println!("Reset plane data for '{game_id}' under {}", data_dir.display());
    Ok(())
}

fn run_seed(args: &[String]) -> anyhow::Result<()> {
    let game_id = args
        .first()
        .ok_or_else(|| anyhow::anyhow!("usage: aether_plane seed <game_id> <package.json>"))?;
    let package_path = args
        .get(1)
        .ok_or_else(|| anyhow::anyhow!("usage: aether_plane seed <game_id> <package.json>"))?;
    let data_dir = env_path("AETHER_PLANE_DATA").unwrap_or_else(|| PathBuf::from("data/plane"));
    let store = PlaneStore::new(&data_dir);
    let manifest = store.seed_package(game_id, PathBuf::from(package_path).as_path())?;
    println!(
        "Seeded '{}' v{} at {}",
        manifest.package_id,
        manifest.package_version,
        data_dir.join(game_id).display()
    );
    Ok(())
}

fn env_path(key: &str) -> Option<PathBuf> {
    std::env::var(key).ok().map(PathBuf::from)
}

/// Load `.env` from the working directory or any parent (repo root when run via `aether dev`).
fn load_dotenv() {
    match dotenvy::dotenv() {
        Ok(path) => eprintln!("Loaded env from {}", path.display()),
        Err(dotenvy::Error::Io(io)) if io.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => eprintln!("Warning: could not load .env: {err}"),
    }
}

fn log_generation_env() {
    let ollama = std::env::var("OLLAMA_MODEL").unwrap_or_else(|_| "(not set → llama3.2)".into());
    let ollama_timeout = ollama_timeout_secs();
    let a1111 = std::env::var("A1111_URL").unwrap_or_else(|_| "http://127.0.0.1:7860".into());
    tracing::info!(%ollama, ollama_timeout_secs = ollama_timeout, %a1111, "Generation env");
}
