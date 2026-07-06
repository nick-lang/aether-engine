//! GamePackage loading and validation.

mod layout;
mod layout_context;
mod merge;
mod tiles;
mod tile_catalog;
mod validate;

use std::fs;
use std::path::{Path, PathBuf};

use aether_core::{EngineError, EngineResult, PackageId, PackageVersion};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMeta {
    pub id: String,
    pub name: String,
    pub version: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GamePackage {
    pub meta: PackageMeta,
    pub scenes: Vec<SceneDef>,
    pub entities: Vec<EntityDef>,
    #[serde(default)]
    pub rooms: Vec<RoomDef>,
    #[serde(default)]
    pub mechanics: Vec<serde_json::Value>,
    #[serde(default)]
    pub content: Vec<serde_json::Value>,
    #[serde(default)]
    pub assets: Vec<serde_json::Value>,
    #[serde(default)]
    pub tile_layers: Vec<TileLayerDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDef {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityDef {
    pub id: String,
    pub scene: String,
    #[serde(default)]
    pub components: EntityComponents,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityComponents {
    #[serde(default)]
    pub transform: Option<TransformDef>,
    #[serde(default)]
    pub player: Option<PlayerDef>,
    #[serde(default)]
    pub collectible: Option<CollectibleDef>,
    #[serde(default)]
    pub win_condition: Option<WinConditionDef>,
    #[serde(default)]
    pub sprite: Option<SpriteDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpriteDef {
    pub asset: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransformDef {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerDef {
    #[serde(default = "default_speed")]
    pub speed: f32,
}

fn default_speed() -> f32 {
    4.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectibleDef {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WinConditionDef {
    /// Single-collectible win (legacy); ignored when `requires_all` is non-empty.
    pub requires_collectible: String,
    /// Win when every listed collectible id has been picked up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires_all: Vec<String>,
}

impl WinConditionDef {
    pub fn required_collectible_ids(&self) -> Vec<String> {
        if !self.requires_all.is_empty() {
            return self.requires_all.clone();
        }
        if self.requires_collectible.trim().is_empty() {
            return Vec::new();
        }
        vec![self.requires_collectible.clone()]
    }
}

impl GamePackage {
    pub fn package_id(&self) -> PackageId {
        PackageId(self.meta.id.clone())
    }

    pub fn version(&self) -> PackageVersion {
        PackageVersion(self.meta.version)
    }

    pub fn default_scene_id(&self) -> EngineResult<&str> {
        self.scenes
            .first()
            .map(|s| s.id.as_str())
            .ok_or_else(|| EngineError::InvalidPackage("package has no scenes".into()))
    }

    pub fn entities_for_scene(&self, scene_id: &str) -> Vec<&EntityDef> {
        self.entities
            .iter()
            .filter(|e| e.scene == scene_id)
            .collect()
    }

    pub fn rooms_for_scene(&self, scene_id: &str) -> Vec<&RoomDef> {
        self.rooms
            .iter()
            .filter(|r| r.scene == scene_id)
            .collect()
    }

    pub fn tile_layers_for_scene(&self, scene_id: &str) -> Vec<&TileLayerDef> {
        self.tile_layers
            .iter()
            .filter(|l| l.scene == scene_id)
            .collect()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomDef {
    pub id: String,
    pub scene: String,
    pub bounds: RoomBoundsDef,
    #[serde(default)]
    pub label: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomBoundsDef {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl RoomBoundsDef {
    pub fn center(&self) -> (f32, f32) {
        (self.x + self.width * 0.5, self.y + self.height * 0.5)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileCellDef {
    pub x: i32,
    pub y: i32,
    pub tile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TileLayerDef {
    pub scene: String,
    pub room_id: String,
    #[serde(default = "default_tile_size")]
    pub tile_size: f32,
    /// Uniform floor: tessellated at render time with edge clipping.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// Sparse overrides or legacy explicit grids (used when `fill` is absent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cells: Vec<TileCellDef>,
}

impl TileLayerDef {
    pub fn uses_fill(&self) -> bool {
        self.fill.as_ref().is_some_and(|s| !s.trim().is_empty())
    }
}

fn default_tile_size() -> f32 {
    16.0
}

impl GamePackage {
    pub fn from_json(text: &str) -> EngineResult<Self> {
        serde_json::from_str(text)
            .map_err(|e| EngineError::InvalidPackage(format!("json parse failed: {e}")))
    }
}

pub fn load_package(path: impl AsRef<Path>) -> EngineResult<GamePackage> {
    let text = fs::read_to_string(path.as_ref())
        .map_err(|e| EngineError::InvalidPackage(format!("read failed: {e}")))?;
    let package = GamePackage::from_json(&text)?;
    validate::validate_package(&package)?;
    Ok(package)
}

pub use layout::{
    attach_room, default_attach_height, default_attach_width, default_main_room_bounds,
    extremal_room_bounds, place_in_room, room_is_beyond, rooms_share_edge, Direction, RoomAnchor,
    DEFAULT_EW_ROOM_WIDTH, DEFAULT_NS_ROOM_HEIGHT, EDGE_INSET,
};
pub use layout_context::LayoutContext;
pub use tile_catalog::{tile_asset_id, tile_rgb};
pub use tiles::{fill_room_grid, tile_world_center, tessellate_room_fill, PlacedTile};
pub use merge::{
    apply_patch_to_package, paint_op_to_layer, patch_invalid_for_package, patch_is_noop_for_package,
    prune_orphan_tile_layers,
};
pub use validate::{validate_package, validate_room};

pub fn package_path_from_arg(path: Option<PathBuf>) -> PathBuf {
    path.unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/minimal_explorer/package.json")
    })
}
