//! Run a GamePackage in a Bevy window.

mod app;
mod plane;

use std::env;
use std::path::PathBuf;

use aether_core::{ENGINE_NAME, ENGINE_VERSION};
use aether_package::{load_package, package_path_from_arg, validate_package, GamePackage};
use app::RunOptions;
use plane::PlaneConfig;

fn main() {
    let args: Vec<String> = env::args().collect();
    let watch = args.iter().any(|a| a == "--watch" || a == "-w");
    let plane_url = flag_value(&args, "--plane");
    let game_id_flag = flag_value(&args, "--game");
    let package_path = package_path_from_arg(non_flag_path(&args));

    println!("{ENGINE_NAME} v{ENGINE_VERSION}");

    let plane = plane_url.map(|base_url| PlaneConfig {
        base_url,
        game_id: game_id_flag.unwrap_or_else(|| {
            load_package(&package_path)
                .map(|p| p.meta.id)
                .unwrap_or_else(|_| "minimal_explorer".into())
        }),
    });

    let package = match load_package_for_session(&package_path, plane.as_ref()) {
        Ok(pkg) => {
            println!(
                "Loaded '{}' ({}) v{}",
                pkg.meta.name, pkg.meta.id, pkg.meta.version
            );
            pkg
        }
        Err(err) => {
            eprintln!("Failed to load package: {err}");
            std::process::exit(1);
        }
    };

    if let Some(ref p) = plane {
        println!("Content plane: {} (game {})", p.base_url, p.game_id);
    }

    app::run(RunOptions {
        package,
        path: package_path,
        watch,
        plane,
    });
}

fn load_package_for_session(
    path: &PathBuf,
    plane: Option<&PlaneConfig>,
) -> Result<GamePackage, String> {
    if let Some(p) = plane {
        let url = format!(
            "{}/packages/{}/canonical",
            p.base_url.trim_end_matches('/'),
            p.game_id
        );
        match reqwest::blocking::get(&url) {
            Ok(res) if res.status().is_success() => {
                let text = res.text().map_err(|e| e.to_string())?;
                let pkg = GamePackage::from_json(&text).map_err(|e| e.to_string())?;
                validate_package(&pkg).map_err(|e| e.to_string())?;
                println!("Using canonical package from content plane");
                return Ok(pkg);
            }
            Ok(res) => {
                eprintln!(
                    "Warning: plane canonical HTTP {} — falling back to {}",
                    res.status(),
                    path.display()
                );
            }
            Err(err) => {
                eprintln!(
                    "Warning: plane canonical fetch failed ({err}) — falling back to {}",
                    path.display()
                );
            }
        }
    }
    load_package(path).map_err(|e| e.to_string())
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn non_flag_path(args: &[String]) -> Option<PathBuf> {
    let mut skip_next = false;
    for arg in args.iter().skip(1) {
        if skip_next {
            skip_next = false;
            continue;
        }
        if arg == "--watch" || arg == "-w" {
            continue;
        }
        if arg == "--plane" || arg == "--game" {
            skip_next = true;
            continue;
        }
        if !arg.starts_with('-') {
            return Some(PathBuf::from(arg));
        }
    }
    None
}
