mod dev;
mod plane_client;
mod plane_cmd;
mod playtest;

use std::env;
use std::path::PathBuf;
use std::process::{self, Command};

use aether_package::{load_package, package_path_from_arg};
use aether_patch::load_patch;

fn usage() -> &'static str {
    r#"Usage:
  aether dev [package.json] [--plane URL] [--watch] [--no-spawn-plane] [--no-open-studio]
  aether validate <package.json>
  aether validate-patch <patch.json> [package.json]
  aether run [package.json] [--watch] [--plane URL] [--game ID]
  aether plane ensure [--spawn] [--wait SECS]
  aether plane health | playtest | smoke | generate | canonical | manifest | candidates
  aether plane publish | rollback | submit | seed | reset
  aether plane [--port PORT]   (run server)
  aether version"#
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        eprintln!("{}", usage());
        process::exit(1);
    };

    match cmd.as_str() {
        "dev" => {
            if args.get(1).is_some_and(|a| a == "--help" || a == "-h") {
                println!("{}", dev::dev_usage());
                return;
            }
            let opts = dev::parse_dev_args(&args[1..]);
            process::exit(dev::run_dev(opts));
        }
        "version" => {
            println!("{} {}", aether_core::ENGINE_NAME, aether_core::ENGINE_VERSION);
        }
        "validate" => {
            let path = package_arg(&args[1..]);
            match load_package(&path) {
                Ok(pkg) => println!(
                    "OK: {} v{} ({} scenes, {} entities)",
                    pkg.meta.id,
                    pkg.meta.version,
                    pkg.scenes.len(),
                    pkg.entities.len()
                ),
                Err(err) => {
                    eprintln!("INVALID: {err}");
                    process::exit(1);
                }
            }
        }
        "validate-patch" => {
            let rest: Vec<_> = args[1..].iter().map(String::as_str).collect();
            let patch_path = PathBuf::from(
                rest.first()
                    .unwrap_or(&"examples/minimal_explorer/patches/add_second_shard.json"),
            );
            let package_path = package_path_from_arg(rest.get(1).map(|p| PathBuf::from(*p)));
            let patch = load_patch(&patch_path).expect("patch");
            let package = load_package(&package_path).expect("package");
            patch.validate_version(package.meta.version).expect("version");
            println!("OK: patch '{}' for package '{}'", patch.patch_id, package.meta.id);
        }
        "run" => {
            let watch = args.iter().any(|a| a == "--watch" || a == "-w");
            let plane = flag_value(&args, "--plane");
            let game = flag_value(&args, "--game");
            let path = package_path_from_args(&args[1..]);
            let mut cmd = Command::new("cargo");
            cmd.args(["run", "-p", "aether_player", "--"]);
            if watch {
                cmd.arg("--watch");
            }
            if let Some(url) = plane {
                cmd.arg("--plane").arg(url);
            }
            if let Some(id) = game {
                cmd.arg("--game").arg(id);
            }
            cmd.arg(path.as_os_str());
            process::exit(cmd.status().unwrap().code().unwrap_or(1));
        }
        "plane" => {
            plane_cmd::run_plane_subcommand(&args[1..]);
        }
        _ => {
            eprintln!("unknown command: {cmd}\n{}", usage());
            process::exit(1);
        }
    }
}

fn package_path_from_args(rest: &[String]) -> PathBuf {
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "--watch" | "-w" => i += 1,
            "--plane" | "--game" => i += 2,
            arg if !arg.starts_with('-') => return PathBuf::from(arg),
            _ => i += 1,
        }
    }
    package_path_from_arg(None)
}

fn package_arg(rest: &[String]) -> PathBuf {
    package_path_from_arg(rest.first().map(PathBuf::from))
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}
