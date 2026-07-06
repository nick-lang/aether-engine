use std::collections::HashSet;
use std::path::Path;

use aether_core::EngineResult;
use aether_package::{tile_rgb, tessellate_room_fill, tile_world_center, validate_room, EntityDef, GamePackage, RoomDef,
    TileLayerDef,
};
use aether_patch::PatchDocument;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::{
    Collectible, PackageEntityId, PackageRoomId, PackageSpawned, PackageTileRoom,
    PlaneAssetResolver, Player, WinCondition, WinProgress,
};

const PLAYER_SIZE: Vec2 = Vec2::new(28.0, 28.0);
const COLLECTIBLE_SIZE: Vec2 = Vec2::new(20.0, 20.0);
/// World size for plane PNG sprites (assets are 64×64 px; scale up for visibility).
const PLANE_SPRITE_SIZE: Vec2 = Vec2::new(48.0, 48.0);
const ROOM_BORDER_THICKNESS: f32 = 3.0;

pub fn spawn_scene(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    package: &GamePackage,
    scene_id: &str,
    assets: Option<&PlaneAssetResolver>,
) -> EngineResult<()> {
    for room in package.rooms_for_scene(scene_id) {
        spawn_room(commands, room);
    }
    for entity in package.entities_for_scene(scene_id) {
        spawn_entity(commands, images, entity, assets);
    }
    for layer in package.tile_layers_for_scene(scene_id) {
        if let Some(room) = package.rooms.iter().find(|r| r.id == layer.room_id) {
            spawn_tile_layer(commands, layer, &room.bounds);
        }
    }
    Ok(())
}

/// Spawn or replace an entity by package id.
pub fn upsert_entity(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    existing: &Query<(Entity, &PackageEntityId), With<PackageSpawned>>,
    def: &EntityDef,
    assets: Option<&PlaneAssetResolver>,
    progress: Option<&mut WinProgress>,
) {
    for (entity, id) in existing.iter() {
        if id.0 == def.id {
            commands.entity(entity).despawn_recursive();
        }
    }
    if let Some(progress) = progress {
        progress.picked_entities.remove(&def.id);
        if let Some(collectible) = &def.components.collectible {
            progress
                .collected
                .retain(|id| id != &collectible.id);
        }
    }
    spawn_entity(commands, images, def, assets);
}

/// Spawn or replace a room outline by package room id.
pub fn upsert_room(
    commands: &mut Commands,
    existing: &Query<(Entity, &PackageRoomId), With<PackageSpawned>>,
    def: &RoomDef,
) {
    for (entity, id) in existing.iter() {
        if id.0 == def.id {
            commands.entity(entity).despawn_recursive();
        }
    }
    spawn_room(commands, def);
}

pub fn despawn_package_entities(
    commands: &mut Commands,
    query: &Query<Entity, With<PackageSpawned>>,
) {
    for entity in query.iter() {
        commands.entity(entity).despawn_recursive();
    }
}

pub fn upsert_tile_layer(
    commands: &mut Commands,
    existing: &Query<(Entity, &PackageTileRoom), With<PackageSpawned>>,
    layer: &TileLayerDef,
    room_bounds: &aether_package::RoomBoundsDef,
) {
    for (entity, room_id) in existing.iter() {
        if room_id.0 == layer.room_id {
            commands.entity(entity).despawn_recursive();
        }
    }
    spawn_tile_layer(commands, layer, room_bounds);
}

pub fn spawn_tile_layer(
    commands: &mut Commands,
    layer: &TileLayerDef,
    room_bounds: &aether_package::RoomBoundsDef,
) {
    if layer.uses_fill() {
        let tile = layer.fill.as_deref().unwrap_or("floor");
        for placed in tessellate_room_fill(room_bounds, tile, layer.tile_size) {
            commands.spawn((
                PackageSpawned,
                PackageTileRoom(layer.room_id.clone()),
                Transform::from_xyz(placed.center_x, placed.center_y, -0.5),
                Visibility::default(),
                Sprite::from_color(
                    tile_color(&placed.tile),
                    Vec2::new(placed.width, placed.height),
                ),
            ));
        }
        return;
    }
    let size = Vec2::splat(layer.tile_size * 0.95);
    for cell in &layer.cells {
        let (wx, wy) = tile_world_center(room_bounds, cell.x, cell.y, layer.tile_size);
        commands.spawn((
            PackageSpawned,
            PackageTileRoom(layer.room_id.clone()),
            Transform::from_xyz(wx, wy, -0.5),
            Visibility::default(),
            Sprite::from_color(tile_color(&cell.tile), size),
        ));
    }
}

fn tile_color(tile: &str) -> Color {
    let [r, g, b] = tile_rgb(tile);
    Color::srgb(r, g, b)
}

fn spawn_room(commands: &mut Commands, room: &RoomDef) {
    let b = &room.bounds;
    let (cx, cy) = b.center();
    let fill = Color::srgba(0.35, 0.62, 0.48, 0.22);
    let border = Color::srgb(0.45, 0.88, 0.62);

    let mut root = commands.spawn((
        PackageSpawned,
        PackageRoomId(room.id.clone()),
        Transform::from_xyz(cx, cy, -2.0),
        Visibility::default(),
    ));

    root.with_children(|parent| {
        parent.spawn((
            Sprite::from_color(fill, Vec2::new(b.width, b.height)),
            Transform::from_xyz(0.0, 0.0, 0.0),
        ));
        for (size, ox, oy) in [
            (Vec2::new(b.width, ROOM_BORDER_THICKNESS), 0.0, b.height * 0.5),
            (Vec2::new(b.width, ROOM_BORDER_THICKNESS), 0.0, -b.height * 0.5),
            (Vec2::new(ROOM_BORDER_THICKNESS, b.height), -b.width * 0.5, 0.0),
            (Vec2::new(ROOM_BORDER_THICKNESS, b.height), b.width * 0.5, 0.0),
        ] {
            parent.spawn((
                Sprite::from_color(border, size),
                Transform::from_xyz(ox, oy, 0.1),
            ));
        }
    });
}

fn spawn_entity(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    def: &EntityDef,
    assets: Option<&PlaneAssetResolver>,
) {
    let transform = def
        .components
        .transform
        .as_ref()
        .map(|t| Transform::from_xyz(t.x, t.y, 0.0))
        .unwrap_or(Transform::from_xyz(0.0, 0.0, 0.0));

    let mut entity = commands.spawn((
        PackageSpawned,
        PackageEntityId(def.id.clone()),
        transform,
        Visibility::default(),
    ));

    if let Some(player) = &def.components.player {
        entity.insert((
            Player {
                speed: player.speed,
            },
            Sprite::from_color(Color::srgb(0.2, 0.55, 0.95), PLAYER_SIZE),
        ));
    }

    if let Some(collectible) = &def.components.collectible {
        entity.insert((
            Collectible {
                id: collectible.id.clone(),
            },
            Sprite::from_color(Color::srgb(0.95, 0.82, 0.15), COLLECTIBLE_SIZE),
        ));
    }

    if let Some(win) = &def.components.win_condition {
        entity.insert(WinCondition {
            requires_collectibles: win.required_collectible_ids(),
        });
    }

    if let Some(sprite) = &def.components.sprite {
        if let Some(resolver) = assets {
            if let Some(path) = resolver.ensure_asset(&sprite.asset) {
                let handle = png_to_image_handle(images, &path);
                entity.insert(Sprite {
                    image: handle,
                    custom_size: Some(PLANE_SPRITE_SIZE),
                    ..default()
                });
            } else {
                entity.insert(Sprite::from_color(
                    Color::srgb(0.85, 0.35, 0.85),
                    Vec2::new(40.0, 40.0),
                ));
            }
        } else {
            entity.insert(Sprite::from_color(
                Color::srgb(0.55, 0.55, 0.6),
                Vec2::new(40.0, 40.0),
            ));
        }
    }
}

fn png_to_image_handle(images: &mut Assets<Image>, path: &Path) -> Handle<Image> {
    let Ok(img) = image::open(path) else {
        return images.add(Image::default());
    };
    let rgba = img.to_rgba8();
    let size = Extent3d {
        width: rgba.width(),
        height: rgba.height(),
        depth_or_array_layers: 1,
    };
    let bevy_image = Image::new(
        size,
        TextureDimension::D2,
        rgba.into_raw(),
        TextureFormat::Rgba8UnormSrgb,
        bevy::render::render_asset::RenderAssetUsages::RENDER_WORLD,
    );
    images.add(bevy_image)
}

/// Apply supported patch ops against the active scene.
pub fn apply_patch_to_scene(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    existing_entities: &Query<(Entity, &PackageEntityId), With<PackageSpawned>>,
    existing_rooms: &Query<(Entity, &PackageRoomId), With<PackageSpawned>>,
    existing_tiles: &Query<(Entity, &PackageTileRoom), With<PackageSpawned>>,
    scene_id: &str,
    patch: &PatchDocument,
    manifest_version: u64,
    assets: Option<&PlaneAssetResolver>,
    mut progress: Option<&mut WinProgress>,
    package: &GamePackage,
) -> EngineResult<()> {
    patch.validate_version(manifest_version)?;
    for op in patch.upsert_room_ops() {
        if op.scene != scene_id {
            continue;
        }
        let def: RoomDef = serde_json::from_value(op.room)
            .map_err(|e| aether_core::EngineError::PatchRejected(e.to_string()))?;
        let scenes: HashSet<&str> = HashSet::from([scene_id]);
        validate_room(&def, &scenes)
            .map_err(|e| aether_core::EngineError::PatchRejected(e.to_string()))?;
        upsert_room(commands, existing_rooms, &def);
    }
    for op in patch.upsert_entity_ops() {
        if op.scene != scene_id {
            continue;
        }
        let def: EntityDef = serde_json::from_value(op.entity)
            .map_err(|e| aether_core::EngineError::PatchRejected(e.to_string()))?;
        upsert_entity(
            commands,
            images,
            existing_entities,
            &def,
            assets,
            progress.as_deref_mut(),
        );
    }
    for op in patch.paint_tiles_ops() {
        if op.scene != scene_id {
            continue;
        }
        let layer = aether_package::paint_op_to_layer(&op).map_err(|e| {
            aether_core::EngineError::PatchRejected(format!("paint_tiles: {e}"))
        })?;
        let Some(bounds) = resolve_room_bounds(package, patch, &op.room_id) else {
            eprintln!(
                "paint_tiles: room '{}' not in package or this patch — skipping visuals",
                op.room_id
            );
            continue;
        };
        upsert_tile_layer(commands, existing_tiles, &layer, &bounds);
    }
    Ok(())
}

fn resolve_room_bounds(
    package: &GamePackage,
    patch: &PatchDocument,
    room_id: &str,
) -> Option<aether_package::RoomBoundsDef> {
    if let Some(room) = package.rooms.iter().find(|r| r.id == room_id) {
        return Some(room.bounds.clone());
    }
    for op in patch.upsert_room_ops() {
        let room: RoomDef = serde_json::from_value(op.room.clone()).ok()?;
        if room.id == room_id {
            return Some(room.bounds);
        }
    }
    None
}
