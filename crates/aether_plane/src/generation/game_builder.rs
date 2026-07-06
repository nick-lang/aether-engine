//! Turn a [`GamePlan`] into a validated [`GamePackage`] and audit patch ops.

use aether_core::EngineResult;
use aether_package::{
    default_main_room_bounds, CollectibleDef, EntityComponents, EntityDef, GamePackage, PackageMeta,
    PlayerDef, RoomBoundsDef, RoomDef, SceneDef, TileLayerDef, TransformDef, WinConditionDef,
};
use aether_patch::PatchDocument;
use serde_json::json;

use super::art::simple_nonce;
use super::game_plan::{GamePlan, RoomBounds};

fn win_condition_for_plan(plan: &GamePlan) -> WinConditionDef {
    let requires_all = if plan.win_requires_all && plan.collectibles.len() > 1 {
        plan.collectibles.iter().map(|c| c.id.clone()).collect()
    } else {
        Vec::new()
    };
    WinConditionDef {
        requires_collectible: plan.win_collectible_id.clone(),
        requires_all,
    }
}

pub fn build_package(game_id: &str, plan: &GamePlan) -> GamePackage {
    let mut rooms = vec![RoomDef {
        id: "starting_area".into(),
        scene: plan.scene_id.clone(),
        bounds: default_main_room_bounds(),
        label: Some(plan.main_room_label.clone()),
    }];

    for extra in &plan.extra_rooms {
        rooms.push(RoomDef {
            id: extra.id.clone(),
            scene: plan.scene_id.clone(),
            bounds: bounds_to_def(extra.bounds),
            label: Some(extra.label.clone()),
        });
    }

    let mut entities = vec![
        EntityDef {
            id: "player".into(),
            scene: plan.scene_id.clone(),
            components: EntityComponents {
                transform: Some(TransformDef { x: 0.0, y: 0.0 }),
                player: Some(PlayerDef { speed: 220.0 }),
                ..Default::default()
            },
        },
        EntityDef {
            id: "win".into(),
            scene: plan.scene_id.clone(),
            components: EntityComponents {
                win_condition: Some(win_condition_for_plan(plan)),
                ..Default::default()
            },
        },
    ];

    for c in &plan.collectibles {
        entities.push(EntityDef {
            id: format!("pickup_{}", c.id),
            scene: plan.scene_id.clone(),
            components: EntityComponents {
                transform: Some(TransformDef { x: c.x, y: c.y }),
                collectible: Some(CollectibleDef { id: c.id.clone() }),
                ..Default::default()
            },
        });
    }

    let tile_layers = default_tile_layers_for_game(&plan.scene_id, &rooms);

    GamePackage {
        meta: PackageMeta {
            id: game_id.to_string(),
            name: plan.title.clone(),
            version: 1,
        },
        scenes: vec![SceneDef {
            id: plan.scene_id.clone(),
            name: Some(plan.scene_name.clone()),
        }],
        entities,
        rooms,
        mechanics: vec![],
        content: vec![],
        assets: vec![],
        tile_layers,
    }
}

fn default_tile_layers_for_game(scene_id: &str, rooms: &[RoomDef]) -> Vec<TileLayerDef> {
    rooms
        .iter()
        .map(|room| {
            let fill = default_game_room_fill(&room.id, room.label.as_deref());
            TileLayerDef {
                scene: scene_id.to_string(),
                room_id: room.id.clone(),
                tile_size: 16.0,
                fill: Some(fill.to_string()),
                cells: Vec::new(),
            }
        })
        .collect()
}

fn default_game_room_fill(room_id: &str, label: Option<&str>) -> &'static str {
    if room_id == "starting_area" {
        return "grass";
    }
    let l = label.unwrap_or("").to_lowercase();
    if l.contains("moss") || (l.contains("grove") && room_id.contains("north")) {
        return "moss";
    }
    if l.contains("dungeon") || l.contains("stone") {
        return "wall";
    }
    if room_id.contains("north") {
        return "moss";
    }
    "grass"
}

/// Attach a generated sprite to the first collectible entity (hero prop).
pub fn attach_sprite_to_primary_collectible(
    package: &mut GamePackage,
    asset_id: &str,
    collectible_id: &str,
) {
    let target_id = format!("pickup_{collectible_id}");
    for entity in &mut package.entities {
        if entity.id == target_id {
            entity.components.sprite = Some(aether_package::SpriteDef {
                asset: asset_id.to_string(),
            });
            return;
        }
    }
}

pub fn package_to_audit_patch(package: &GamePackage, patch_id: &str) -> PatchDocument {
    let scene_id = package
        .default_scene_id()
        .unwrap_or("main");
    let mut ops = Vec::new();

    for room in &package.rooms {
        ops.push(json!({
            "op": "upsert_room",
            "scene": scene_id,
            "room": room,
        }));
    }

    for entity in &package.entities {
        ops.push(json!({
            "op": "upsert_entity",
            "scene": scene_id,
            "entity": entity,
        }));
    }

    for layer in &package.tile_layers {
        if let Some(fill) = layer.fill.as_ref().filter(|s| !s.trim().is_empty()) {
            ops.push(json!({
                "op": "paint_tiles",
                "scene": scene_id,
                "room_id": layer.room_id,
                "tile_size": layer.tile_size,
                "fill": fill,
            }));
        }
    }

    PatchDocument {
        patch_id: patch_id.to_string(),
        base_version: package.meta.version,
        ops,
    }
}

pub fn validate_built_package(package: &GamePackage) -> EngineResult<()> {
    aether_package::validate_package(package)
}

fn bounds_to_def(b: RoomBounds) -> RoomBoundsDef {
    RoomBoundsDef {
        x: b.x,
        y: b.y,
        width: b.width,
        height: b.height,
    }
}

pub fn new_game_patch_id() -> String {
    format!("game_{}", simple_nonce())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::game_plan::parse_game_prompt;

    #[test]
    fn built_package_is_playable() {
        let plan = parse_game_prompt("treasure hunt — collect the golden shard");
        let package = build_package("minimal_explorer", &plan);
        validate_built_package(&package).expect("valid game");
        assert!(package.entities.iter().any(|e| e.components.player.is_some()));
        assert!(
            package
                .entities
                .iter()
                .any(|e| e.components.win_condition.is_some())
        );
        assert!(
            package.tile_layers.iter().any(|l| l.fill.as_deref() == Some("grass")),
            "main area should have grass fill"
        );
    }

    #[test]
    fn multi_shard_plan_uses_requires_all_win() {
        let plan = parse_game_prompt("A game with two shards — collect all");
        let package = build_package("multi_shard", &plan);
        validate_built_package(&package).expect("valid multi-shard game");
        let win = package
            .entities
            .iter()
            .find_map(|e| e.components.win_condition.as_ref())
            .expect("win entity");
        assert_eq!(win.requires_all.len(), 2);
        assert!(win.requires_all.contains(&"golden_shard".to_string()));
        assert!(win.requires_all.contains(&"silver_shard".to_string()));
    }
}
