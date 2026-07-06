//! Semantic validation for [`GamePackage`].

use std::collections::HashSet;

use aether_core::{EngineError, EngineResult};

use crate::{GamePackage, RoomDef};

pub fn validate_package(package: &GamePackage) -> EngineResult<()> {
    if package.meta.id.trim().is_empty() {
        return Err(EngineError::InvalidPackage("meta.id must not be empty".into()));
    }
    if package.meta.name.trim().is_empty() {
        return Err(EngineError::InvalidPackage("meta.name must not be empty".into()));
    }
    if package.scenes.is_empty() {
        return Err(EngineError::InvalidPackage("at least one scene is required".into()));
    }

    let scene_ids: HashSet<&str> = package.scenes.iter().map(|s| s.id.as_str()).collect();
    if scene_ids.len() != package.scenes.len() {
        return Err(EngineError::InvalidPackage("duplicate scene ids".into()));
    }

    let mut entity_ids = HashSet::new();
    let mut collectible_ids = HashSet::new();

    for entity in &package.entities {
        if !entity_ids.insert(entity.id.as_str()) {
            return Err(EngineError::InvalidPackage(format!(
                "duplicate entity id '{}'",
                entity.id
            )));
        }
        if !scene_ids.contains(entity.scene.as_str()) {
            return Err(EngineError::InvalidPackage(format!(
                "entity '{}' references unknown scene '{}'",
                entity.id, entity.scene
            )));
        }

        let components = &entity.components;
        let has_role = components.player.is_some()
            || components.collectible.is_some()
            || components.win_condition.is_some()
            || components.sprite.is_some();
        if !has_role {
            return Err(EngineError::InvalidPackage(format!(
                "entity '{}' needs player, collectible, or win_condition",
                entity.id
            )));
        }

        if let Some(collectible) = &components.collectible {
            if collectible.id.trim().is_empty() {
                return Err(EngineError::InvalidPackage(format!(
                    "entity '{}' has empty collectible id",
                    entity.id
                )));
            }
            collectible_ids.insert(collectible.id.as_str());
        }
    }

    for entity in &package.entities {
        if let Some(win) = &entity.components.win_condition {
            for cid in win.required_collectible_ids() {
                if !collectible_ids.contains(cid.as_str()) {
                    return Err(EngineError::InvalidPackage(format!(
                        "win_condition references unknown collectible '{cid}'"
                    )));
                }
            }
        }
    }

    let scenes_with_player: HashSet<&str> = package
        .entities
        .iter()
        .filter(|e| e.components.player.is_some())
        .map(|e| e.scene.as_str())
        .collect();

    for scene in &package.scenes {
        if !scenes_with_player.contains(scene.id.as_str()) {
            return Err(EngineError::InvalidPackage(format!(
                "scene '{}' has no player entity",
                scene.id
            )));
        }
    }

    let mut room_ids = HashSet::new();
    for room in &package.rooms {
        if !room_ids.insert(room.id.as_str()) {
            return Err(EngineError::InvalidPackage(format!(
                "duplicate room id '{}'",
                room.id
            )));
        }
        validate_room(room, &scene_ids)?;
    }

    for layer in &package.tile_layers {
        if !scene_ids.contains(layer.scene.as_str()) {
            return Err(EngineError::InvalidPackage(format!(
                "tile layer for room '{}' references unknown scene '{}'",
                layer.room_id, layer.scene
            )));
        }
        if !room_ids.contains(layer.room_id.as_str()) {
            return Err(EngineError::InvalidPackage(format!(
                "tile layer references unknown room '{}'",
                layer.room_id
            )));
        }
        if !layer.uses_fill() && layer.cells.is_empty() {
            return Err(EngineError::InvalidPackage(format!(
                "tile layer for room '{}' needs fill or cells",
                layer.room_id
            )));
        }
    }

    Ok(())
}

pub fn validate_room(room: &RoomDef, scene_ids: &HashSet<&str>) -> EngineResult<()> {
    if room.id.trim().is_empty() {
        return Err(EngineError::InvalidPackage("room id must not be empty".into()));
    }
    if !scene_ids.contains(room.scene.as_str()) {
        return Err(EngineError::InvalidPackage(format!(
            "room '{}' references unknown scene '{}'",
            room.id, room.scene
        )));
    }
    if room.bounds.width <= 0.0 || room.bounds.height <= 0.0 {
        return Err(EngineError::InvalidPackage(format!(
            "room '{}' bounds width and height must be positive",
            room.id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load_package;
    use crate::{
        CollectibleDef, EntityComponents, EntityDef, GamePackage, PackageMeta, PlayerDef,
        SceneDef, TransformDef, WinConditionDef,
    };

    #[test]
    fn minimal_explorer_is_valid() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let package = load_package(&path).expect("load example");
        validate_package(&package).expect("example should validate");
    }

    #[test]
    fn requires_all_win_references_all_collectibles() {
        let package = GamePackage {
            meta: PackageMeta {
                id: "test".into(),
                name: "Test".into(),
                version: 1,
            },
            scenes: vec![SceneDef {
                id: "main".into(),
                name: None,
            }],
            entities: vec![
                EntityDef {
                    id: "player".into(),
                    scene: "main".into(),
                    components: EntityComponents {
                        player: Some(PlayerDef { speed: 220.0 }),
                        ..Default::default()
                    },
                },
                EntityDef {
                    id: "win".into(),
                    scene: "main".into(),
                    components: EntityComponents {
                        win_condition: Some(WinConditionDef {
                            requires_collectible: String::new(),
                            requires_all: vec!["a".into(), "b".into()],
                        }),
                        ..Default::default()
                    },
                },
                EntityDef {
                    id: "pickup_a".into(),
                    scene: "main".into(),
                    components: EntityComponents {
                        transform: Some(TransformDef { x: 0.0, y: 0.0 }),
                        collectible: Some(CollectibleDef { id: "a".into() }),
                        ..Default::default()
                    },
                },
                EntityDef {
                    id: "pickup_b".into(),
                    scene: "main".into(),
                    components: EntityComponents {
                        transform: Some(TransformDef { x: 1.0, y: 1.0 }),
                        collectible: Some(CollectibleDef { id: "b".into() }),
                        ..Default::default()
                    },
                },
            ],
            rooms: vec![],
            mechanics: vec![],
            content: vec![],
            assets: vec![],
            tile_layers: vec![],
        };
        validate_package(&package).expect("requires_all win should validate");
    }
}
