use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use aether_ecs::{
    apply_patch_to_scene, spawn_scene, AetherEcsPlugin, PackageEntityId, PackageRoomId,
    PackageSpawned, PackageTileRoom, PlaneAssetResolver, WinProgress, WinUiText,
};
use aether_package::GamePackage;
use aether_patch::{load_patch, PatchDocument};
use aether_render::AetherRenderPlugin;
use aether_sim::AetherSimPlugin;
use bevy::prelude::*;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::plane::{fetch_canonical, spawn_subscriber, PlaneConfig, PlaneSync};

pub struct RunOptions {
    pub package: GamePackage,
    pub path: PathBuf,
    pub watch: bool,
    pub plane: Option<PlaneConfig>,
}

#[derive(Resource)]
struct PackageState {
    path: PathBuf,
    package: GamePackage,
    scene_id: String,
}

#[derive(Resource, Default)]
struct ReloadRequested(bool);

#[derive(Resource, Clone)]
struct HotReloadPending(Arc<AtomicBool>);

#[derive(Resource, Clone)]
struct PlanePatchQueue(Arc<Mutex<Vec<PatchDocument>>>);

#[derive(Resource, Clone)]
struct PlaneCanonicalReload(Arc<AtomicBool>);

#[derive(Resource, Clone)]
struct ActivePlane(PlaneConfig);

pub fn run(options: RunOptions) {
    let scene_id = options
        .package
        .default_scene_id()
        .expect("validated package")
        .to_string();

    let mut app = App::new();
    app.insert_resource(ClearColor(Color::srgb(0.06, 0.07, 0.1)))
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: format!("Aether — {}", options.package.meta.name),
                resolution: (960., 540.).into(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins((AetherEcsPlugin, AetherSimPlugin, AetherRenderPlugin))
        .insert_resource(PackageState {
            path: options.path.clone(),
            package: options.package,
            scene_id,
        })
        .insert_resource(ReloadRequested::default())
        .add_systems(Startup, bootstrap_scene)
        .add_systems(
            Update,
            (
                handle_reload,
                reload_from_plane,
                apply_demo_patch,
                apply_demo_room_patch,
                apply_plane_patches,
            ),
        );

    if let Some(plane) = options.plane {
        let sync = PlaneSync {
            patches: Arc::new(Mutex::new(Vec::new())),
            reload_canonical: Arc::new(AtomicBool::new(false)),
        };
        spawn_subscriber(plane.clone(), sync.clone());
        app.insert_resource(PlanePatchQueue(sync.patches));
        app.insert_resource(PlaneCanonicalReload(sync.reload_canonical));
        app.insert_resource(ActivePlane(plane.clone()));
        let cache_dir = std::env::temp_dir()
            .join("aether_assets")
            .join(&plane.game_id);
        app.insert_resource(PlaneAssetResolver {
            base_url: plane.base_url,
            game_id: plane.game_id,
            cache_dir,
        });
    }

    if options.watch {
        let pending = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&pending);
        let path = options.path.clone();
        let mut watcher = RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Any
                    ) {
                        flag.store(true, Ordering::Relaxed);
                    }
                }
            },
            notify::Config::default(),
        )
        .expect("file watcher");
        watcher
            .watch(&path, RecursiveMode::NonRecursive)
            .expect("watch package file");

        let _watcher = watcher;

        app.insert_resource(HotReloadPending(pending))
            .add_systems(Update, poll_hot_reload);

        app.run();
    } else {
        app.run();
    }
}

fn bootstrap_scene(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    state: Res<PackageState>,
    plane: Option<Res<PlanePatchQueue>>,
    plane_assets: Option<Res<PlaneAssetResolver>>,
    mut progress: ResMut<WinProgress>,
    mut ui: ResMut<WinUiText>,
) {
    spawn_scene(
        &mut commands,
        &mut images,
        &state.package,
        &state.scene_id,
        plane_assets.as_deref(),
    )
    .expect("spawn initial scene");
        progress.collected.clear();
        progress.picked_entities.clear();
        progress.won = false;
    ui.0 = if plane.is_some() {
        "WASD: move | Studio publish → live | P: shard patch | R: room patch".into()
    } else {
        "WASD: move | P: local patch demo | --plane for content plane".into()
    };
}

fn reload_from_plane(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<PackageState>,
    plane: Option<Res<ActivePlane>>,
    reload: Option<Res<PlaneCanonicalReload>>,
    plane_assets: Option<Res<PlaneAssetResolver>>,
    mut progress: ResMut<WinProgress>,
    mut ui: ResMut<WinUiText>,
    spawned: Query<Entity, With<PackageSpawned>>,
) {
    let Some(reload) = reload else {
        return;
    };
    if !reload.0.swap(false, Ordering::Relaxed) {
        return;
    }
    let Some(plane) = plane else {
        return;
    };

    match fetch_canonical(&plane.0) {
        Ok(package) => {
            state.scene_id = package
                .default_scene_id()
                .expect("scene")
                .to_string();
            state.package = package;
        }
        Err(err) => {
            ui.0 = format!("Canonical reload failed: {err}");
            eprintln!("Canonical reload failed: {err}");
            return;
        }
    }

    for entity in &spawned {
        commands.entity(entity).despawn_recursive();
    }
    spawn_scene(
        &mut commands,
        &mut images,
        &state.package,
        &state.scene_id,
        plane_assets.as_deref(),
    )
    .expect("respawn from plane");
        progress.collected.clear();
        progress.picked_entities.clear();
        progress.won = false;
    ui.0 = "World reloaded from plane (rollback)".into();
    println!("Reloaded canonical package from content plane");
}

fn handle_reload(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<PackageState>,
    mut reload: ResMut<ReloadRequested>,
    mut progress: ResMut<WinProgress>,
    mut ui: ResMut<WinUiText>,
    plane_assets: Option<Res<PlaneAssetResolver>>,
    spawned: Query<Entity, With<PackageSpawned>>,
) {
    if !reload.0 {
        return;
    }
    reload.0 = false;

    match aether_package::load_package(&state.path) {
        Ok(package) => {
            state.scene_id = package
                .default_scene_id()
                .expect("scene")
                .to_string();
            state.package = package;
        }
        Err(err) => {
            eprintln!("Hot reload skipped (invalid package): {err}");
            return;
        }
    }

    for entity in &spawned {
        commands.entity(entity).despawn_recursive();
    }
    spawn_scene(
        &mut commands,
        &mut images,
        &state.package,
        &state.scene_id,
        plane_assets.as_deref(),
    )
    .expect("respawn");
        progress.collected.clear();
        progress.picked_entities.clear();
        progress.won = false;
    ui.0 = "Package reloaded".into();
    eprintln!("Hot reloaded {}", state.path.display());
}

fn apply_plane_patches(
    mut state: ResMut<PackageState>,
    queue: Option<Res<PlanePatchQueue>>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    plane_assets: Option<Res<PlaneAssetResolver>>,
    mut ui: ResMut<WinUiText>,
    mut progress: ResMut<WinProgress>,
    existing_entities: Query<(Entity, &PackageEntityId), With<PackageSpawned>>,
    existing_rooms: Query<(Entity, &PackageRoomId), With<PackageSpawned>>,
    existing_tiles: Query<(Entity, &PackageTileRoom), With<PackageSpawned>>,
) {
    let Some(queue) = queue else {
        return;
    };
    let mut pending = match queue.0.lock() {
        Ok(p) => p,
        Err(_) => return,
    };
    if pending.is_empty() {
        return;
    }

    let patches: Vec<PatchDocument> = pending.drain(..).collect();
    drop(pending);

    for patch in patches {
        match apply_patch_to_scene(
            &mut commands,
            &mut images,
            &existing_entities,
            &existing_rooms,
            &existing_tiles,
            &state.scene_id,
            &patch,
            state.package.meta.version,
            plane_assets.as_deref(),
            Some(&mut progress),
            &state.package,
        ) {
            Ok(()) => {
                if let Err(err) =
                    aether_package::apply_patch_to_package(&mut state.package, &patch)
                {
                    eprintln!("Warning: in-memory package merge failed: {err}");
                }
                let kinds: Vec<&str> = patch
                    .ops
                    .iter()
                    .filter_map(|op| op.get("op").and_then(|v| v.as_str()))
                    .collect();
                ui.0 = format!("Live patch '{}' ({})", patch.patch_id, kinds.join(", "));
                println!(
                    "Applied plane patch '{}' ops=[{}]",
                    patch.patch_id,
                    kinds.join(", ")
                );
            }
            Err(err) => {
                ui.0 = format!("Plane patch rejected: {err}");
                eprintln!("Plane patch rejected: {err}");
            }
        }
    }
}

fn apply_demo_patch(
    keyboard: Res<ButtonInput<KeyCode>>,
    state: Res<PackageState>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    plane_assets: Option<Res<PlaneAssetResolver>>,
    mut ui: ResMut<WinUiText>,
    mut progress: ResMut<WinProgress>,
    existing_entities: Query<(Entity, &PackageEntityId), With<PackageSpawned>>,
    existing_rooms: Query<(Entity, &PackageRoomId), With<PackageSpawned>>,
    existing_tiles: Query<(Entity, &PackageTileRoom), With<PackageSpawned>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyP) {
        return;
    }

    let patch_path = state
        .path
        .parent()
        .map(|dir| dir.join("patches/add_second_shard.json"));

    let Some(patch_path) = patch_path else {
        ui.0 = "Patch failed: package path has no parent directory".into();
        return;
    };

    match load_patch(&patch_path) {
        Ok(patch) => {
            if let Err(err) = apply_patch_to_scene(
                &mut commands,
                &mut images,
                &existing_entities,
                &existing_rooms,
                &existing_tiles,
                &state.scene_id,
                &patch,
                state.package.meta.version,
                plane_assets.as_deref(),
                Some(&mut progress),
                &state.package,
            ) {
                ui.0 = format!("Patch rejected: {err}");
            } else {
                ui.0 = "Local patch applied (northern shard)".into();
            }
        }
        Err(err) => ui.0 = format!("Patch load failed: {err}"),
    }
}

fn apply_demo_room_patch(
    keyboard: Res<ButtonInput<KeyCode>>,
    state: Res<PackageState>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    plane_assets: Option<Res<PlaneAssetResolver>>,
    mut ui: ResMut<WinUiText>,
    existing_entities: Query<(Entity, &PackageEntityId), With<PackageSpawned>>,
    existing_rooms: Query<(Entity, &PackageRoomId), With<PackageSpawned>>,
    existing_tiles: Query<(Entity, &PackageTileRoom), With<PackageSpawned>>,
) {
    if !keyboard.just_pressed(KeyCode::KeyR) {
        return;
    }

    let patch_path = state
        .path
        .parent()
        .map(|dir| dir.join("patches/add_north_room.json"));

    let Some(patch_path) = patch_path else {
        ui.0 = "Patch failed: package path has no parent directory".into();
        return;
    };

    match load_patch(&patch_path) {
        Ok(patch) => {
            if let Err(err) = apply_patch_to_scene(
                &mut commands,
                &mut images,
                &existing_entities,
                &existing_rooms,
                &existing_tiles,
                &state.scene_id,
                &patch,
                state.package.meta.version,
                plane_assets.as_deref(),
                None,
                &state.package,
            ) {
                ui.0 = format!("Patch rejected: {err}");
            } else {
                ui.0 = "Local patch applied (north room)".into();
            }
        }
        Err(err) => ui.0 = format!("Patch load failed: {err}"),
    }
}

fn poll_hot_reload(pending: Res<HotReloadPending>, mut reload: ResMut<ReloadRequested>) {
    if pending.0.swap(false, Ordering::Relaxed) {
        reload.0 = true;
    }
}
