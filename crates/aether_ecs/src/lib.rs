//! Spawn entities from a [`GamePackage`] into a Bevy world.

mod spawn;

use std::path::PathBuf;

pub use spawn::*;

use bevy::prelude::*;

pub struct AetherEcsPlugin;

impl Plugin for AetherEcsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WinProgress>()
            .init_resource::<WinUiText>()
            .add_systems(Update, sync_win_progress);
    }
}

/// Logical entity id from the package (for patches and hot reload).
#[derive(Component, Clone)]
pub struct PackageEntityId(pub String);

/// Logical room id from the package (rectangular bounds + outline).
#[derive(Component, Clone)]
pub struct PackageRoomId(pub String);

#[derive(Component)]
pub struct Player {
    pub speed: f32,
}

#[derive(Component)]
pub struct Collectible {
    pub id: String,
}

#[derive(Component)]
pub struct WinCondition {
    pub requires_collectibles: Vec<String>,
}

/// Marks entities spawned from package data (despawned on reload).
#[derive(Component)]
pub struct PackageSpawned;

/// Tiles belong to a package room (for live `paint_tiles` refresh).
#[derive(Component, Clone)]
pub struct PackageTileRoom(pub String);

/// Fetches PNG assets from the content plane into a local cache (player `--plane`).
#[derive(Resource, Clone)]
pub struct PlaneAssetResolver {
    pub base_url: String,
    pub game_id: String,
    pub cache_dir: PathBuf,
}

impl PlaneAssetResolver {
    pub fn ensure_asset(&self, asset_id: &str) -> Option<PathBuf> {
        let path = self.cache_dir.join(format!("{asset_id}.png"));
        if path.is_file() {
            return Some(path);
        }
        let url = format!(
            "{}/packages/{}/assets/{}",
            self.base_url.trim_end_matches('/'),
            self.game_id,
            asset_id
        );
        let bytes = reqwest::blocking::get(&url).ok()?.bytes().ok()?;
        std::fs::create_dir_all(&self.cache_dir).ok()?;
        std::fs::write(&path, bytes).ok()?;
        Some(path)
    }
}

#[derive(Resource, Default)]
pub struct WinProgress {
    /// Collectible ids satisfied for win conditions.
    pub collected: Vec<String>,
    /// Entity ids already picked up (avoids duplicate collectible.id blocking other entities).
    pub picked_entities: std::collections::HashSet<String>,
    pub won: bool,
}

fn sync_win_progress(progress: Res<WinProgress>, mut text: ResMut<WinUiText>) {
    if progress.won {
        text.0 = format!(
            "Goal complete — keep exploring! Collected: {}",
            progress.collected.join(", ")
        );
    } else if !progress.collected.is_empty() {
        text.0 = format!("Collected: {}", progress.collected.join(", "));
    }
}

#[derive(Resource, Default)]
pub struct WinUiText(pub String);
