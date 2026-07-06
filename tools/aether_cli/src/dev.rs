//! `aether dev` — one command to seed (if needed), connect to plane, and run the player.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use aether_package::package_path_from_arg;

#[derive(Debug, Clone)]
pub struct DevOptions {
    pub plane_url: String,
    pub package_path: PathBuf,
    pub game_id: Option<String>,
    pub watch: bool,
    pub spawn_plane: bool,
    pub skip_seed: bool,
    pub open_studio: bool,
}

impl Default for DevOptions {
    fn default() -> Self {
        Self {
            plane_url: std::env::var("AETHER_PLANE_URL")
                .unwrap_or_else(|_| "http://127.0.0.1:8787".into()),
            package_path: package_path_from_arg(None),
            game_id: None,
            watch: false,
            spawn_plane: true,
            skip_seed: false,
            open_studio: true,
        }
    }
}

pub fn parse_dev_args(args: &[String]) -> DevOptions {
    let mut opts = DevOptions::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--plane" => {
                if let Some(url) = args.get(i + 1) {
                    opts.plane_url = url.clone();
                    i += 2;
                    continue;
                }
            }
            "--game" => {
                if let Some(id) = args.get(i + 1) {
                    opts.game_id = Some(id.clone());
                    i += 2;
                    continue;
                }
            }
            "--watch" | "-w" => {
                opts.watch = true;
                i += 1;
                continue;
            }
            "--no-spawn-plane" => {
                opts.spawn_plane = false;
                i += 1;
                continue;
            }
            "--skip-seed" => {
                opts.skip_seed = true;
                i += 1;
                continue;
            }
            "--no-open-studio" => {
                opts.open_studio = false;
                i += 1;
                continue;
            }
            arg if !arg.starts_with('-') => {
                opts.package_path = PathBuf::from(arg);
                i += 1;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    opts
}

pub fn run_dev(opts: DevOptions) -> i32 {
    let package_path = opts.package_path.canonicalize().unwrap_or(opts.package_path.clone());
    let game_id = opts
        .game_id
        .clone()
        .or_else(|| game_id_from_package(&package_path).ok())
        .unwrap_or_else(|| "minimal_explorer".into());

    println!("Aether dev");
    println!("  plane:   {}", opts.plane_url);
    println!("  game:    {game_id}");
    println!("  package: {}", package_path.display());

    if !plane_healthy(&opts.plane_url) {
        if opts.spawn_plane {
            println!("\nContent plane is not running — starting it in a new window…");
            if let Err(err) = spawn_plane_window() {
                eprintln!("Failed to start plane: {err}");
                print_manual_plane_hint();
                return 1;
            }
        } else {
            print_manual_plane_hint();
            return 1;
        }

        if !wait_for_plane(&opts.plane_url, 45) {
            eprintln!("Timed out waiting for content plane at {}", opts.plane_url);
            return 1;
        }
        println!("Content plane is up.");
    } else {
        println!("\nContent plane already running.");
    }

    if !opts.skip_seed {
        let data_root = std::env::var("AETHER_PLANE_DATA").unwrap_or_else(|_| "data/plane".into());
        let manifest_path = PathBuf::from(&data_root).join(&game_id).join("manifest.json");
        if !manifest_path.is_file() {
            println!("Seeding canonical package for '{game_id}'…");
            let status = Command::new("cargo")
                .args([
                    "run",
                    "-p",
                    "aether_plane",
                    "--",
                    "seed",
                    &game_id,
                    package_path.to_str().expect("utf-8 path"),
                ])
                .stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit())
                .status();
            match status {
                Ok(s) if s.success() => println!("Seed OK."),
                Ok(s) => {
                    eprintln!("Seed failed (exit {:?}).", s.code());
                    return s.code().unwrap_or(1);
                }
                Err(err) => {
                    eprintln!("Failed to run seed: {err}");
                    return 1;
                }
            }
        } else {
            println!("Canonical package already seeded ({})", manifest_path.display());
        }
    }

    let studio_url = format!("{}/studio/", opts.plane_url.trim_end_matches('/'));
    if opts.open_studio {
        match open_in_browser(&studio_url) {
            Ok(()) => println!("\nOpened Studio in your browser."),
            Err(err) => eprintln!("\nCould not open browser ({err}). Studio: {studio_url}"),
        }
    } else {
        println!("\nStudio: {studio_url}");
    }

    println!("Starting player (close window or Ctrl+C to exit)…\n");

    let mut cmd = Command::new("cargo");
    cmd.args(["run", "-p", "aether_player", "--"]);
    if opts.watch {
        cmd.arg("--watch");
    }
    cmd.arg("--plane").arg(&opts.plane_url);
    cmd.arg(&package_path);

    match cmd.status() {
        Ok(s) => s.code().unwrap_or(1),
        Err(err) => {
            eprintln!("Failed to start player: {err}");
            1
        }
    }
}

fn game_id_from_package(path: &Path) -> Result<String, String> {
    let pkg = aether_package::load_package(path).map_err(|e| e.to_string())?;
    Ok(pkg.meta.id)
}

fn plane_healthy(base_url: &str) -> bool {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let client = match reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(_) => return false,
    };
    client
        .get(&url)
        .send()
        .ok()
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

fn wait_for_plane(base_url: &str, seconds: u64) -> bool {
    for _ in 0..seconds {
        if plane_healthy(base_url) {
            return true;
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    false
}

fn spawn_plane_window() -> Result<(), String> {
    #[cfg(windows)]
    {
        let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
        Command::new("cmd")
            .args([
                "/C",
                "start",
                "Aether Plane",
                "cmd",
                "/k",
                "cargo run -p aether_plane",
            ])
            .current_dir(cwd)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Command::new("cargo")
            .args(["run", "-p", "aether_plane"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn open_in_browser(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        Command::new("cmd")
            .args(["/C", "start", "", url])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = url;
        return Err("unsupported platform".into());
    }
    Ok(())
}

fn print_manual_plane_hint() {
    eprintln!("\nStart the content plane in another terminal, then run dev again:");
    eprintln!("  cargo run -p aether_plane");
    eprintln!("\nOr use: cargo run -p aether_cli -- dev   (spawns plane automatically on Windows)");
}

pub fn dev_usage() -> &'static str {
    r#"  aether dev [package.json] [options]

  One-shot local session: ensure plane is up, seed if needed, run player with --plane.

  Options:
    --plane URL       Content plane base URL (default http://127.0.0.1:8787 or AETHER_PLANE_URL)
    --game ID         Game id for seed check (default: package meta.id)
    --watch, -w       Hot-reload package file in player
    --no-spawn-plane  Fail if plane is down instead of opening a new window
    --skip-seed       Do not run seed even if canonical store is missing
    --no-open-studio  Do not open /studio/ in the default browser

  Examples:
    cargo run -p aether_cli -- dev
    cargo run -p aether_cli -- dev examples/minimal_explorer/package.json --watch
    cargo run -p aether_cli -- dev --no-spawn-plane"#
}
