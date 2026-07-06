//! Headless checks on a canonical [`GamePackage`] (no Bevy window).

use std::collections::HashSet;

use aether_package::{validate_package, GamePackage, RoomBoundsDef};

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlaytestReport {
    pub ok: bool,
    pub game_id: String,
    pub scene_id: String,
    pub room_count: usize,
    pub entity_count: usize,
    pub tile_layer_count: usize,
    pub collectible_count: usize,
    pub issues: Vec<String>,
    pub notes: Vec<String>,
}

pub fn playtest_package(package: &GamePackage) -> PlaytestReport {
    let mut issues = Vec::new();
    let mut notes = Vec::new();

    let scene_id = match package.default_scene_id() {
        Ok(s) => s.to_string(),
        Err(e) => {
            issues.push(e.to_string());
            "main".into()
        }
    };

    if let Err(e) = validate_package(package) {
        issues.push(format!("validate_package: {e}"));
    }

    check_duplicate_ids(package, &mut issues);

    let collectible_ids: HashSet<String> = package
        .entities
        .iter()
        .filter_map(|e| e.components.collectible.as_ref().map(|c| c.id.clone()))
        .collect();

    check_win_condition(package, &collectible_ids, &mut issues);
    check_room_overlaps(package, &mut issues);

    let player = package
        .entities
        .iter()
        .find(|e| e.components.player.is_some());
    match player {
        Some(p) => {
            if let Some(t) = &p.components.transform {
                if !point_in_any_room(&package.rooms, t.x, t.y) {
                    notes.push(format!(
                        "player @ ({}, {}) is outside all room bounds (walkable overworld OK for now)",
                        t.x, t.y
                    ));
                }
            } else {
                issues.push("player entity missing transform".into());
            }
        }
        None => issues.push("no player entity".into()),
    }

    let collectibles: Vec<_> = package
        .entities
        .iter()
        .filter(|e| e.components.collectible.is_some())
        .collect();

    for e in &collectibles {
        let Some(t) = &e.components.transform else {
            issues.push(format!("collectible entity '{}' missing transform", e.id));
            continue;
        };
        if !point_in_any_room(&package.rooms, t.x, t.y) {
            issues.push(format!(
                "collectible '{}' @ ({}, {}) outside all rooms",
                e.id, t.x, t.y
            ));
        }
    }

    for layer in &package.tile_layers {
        if !package.rooms.iter().any(|r| r.id == layer.room_id) {
            issues.push(format!(
                "tile layer references missing room '{}'",
                layer.room_id
            ));
        }
        if !layer.uses_fill() && layer.cells.is_empty() {
            issues.push(format!(
                "tile layer for '{}' has no fill or cells",
                layer.room_id
            ));
        }
    }

    if package.rooms.is_empty() {
        notes.push("no rooms defined".into());
    }

    let win = package
        .entities
        .iter()
        .any(|e| e.components.win_condition.is_some());
    if !win {
        issues.push("no win_condition entity".into());
    }

    PlaytestReport {
        ok: issues.is_empty(),
        game_id: package.meta.id.clone(),
        scene_id,
        room_count: package.rooms.len(),
        entity_count: package.entities.len(),
        tile_layer_count: package.tile_layers.len(),
        collectible_count: collectibles.len(),
        issues,
        notes,
    }
}

fn check_duplicate_ids(package: &GamePackage, issues: &mut Vec<String>) {
    let mut entity_ids = HashSet::new();
    for e in &package.entities {
        if !entity_ids.insert(e.id.clone()) {
            issues.push(format!("duplicate entity id '{}'", e.id));
        }
    }
    let mut room_ids = HashSet::new();
    for r in &package.rooms {
        if !room_ids.insert(r.id.clone()) {
            issues.push(format!("duplicate room id '{}'", r.id));
        }
    }
    let mut collectible_ids = HashSet::new();
    for e in &package.entities {
        if let Some(c) = &e.components.collectible {
            if !collectible_ids.insert(c.id.clone()) {
                issues.push(format!("duplicate collectible id '{}'", c.id));
            }
        }
    }
}

fn check_win_condition(
    package: &GamePackage,
    collectible_ids: &HashSet<String>,
    issues: &mut Vec<String>,
) {
    for e in &package.entities {
        if let Some(win) = &e.components.win_condition {
            for cid in win.required_collectible_ids() {
                if !collectible_ids.contains(cid.as_str()) {
                    issues.push(format!(
                        "win_condition references unknown collectible '{cid}'"
                    ));
                }
            }
        }
    }
}

fn check_room_overlaps(package: &GamePackage, issues: &mut Vec<String>) {
    let rooms = &package.rooms;
    for i in 0..rooms.len() {
        for j in (i + 1)..rooms.len() {
            let a = &rooms[i];
            let b = &rooms[j];
            if a.scene != b.scene {
                continue;
            }
            if bounds_overlap(&a.bounds, &b.bounds) {
                issues.push(format!(
                    "rooms '{}' and '{}' overlap (likely stacked rects, not nested)",
                    a.id, b.id
                ));
            }
        }
    }
}

fn bounds_overlap(a: &RoomBoundsDef, b: &RoomBoundsDef) -> bool {
    a.x < b.x + b.width
        && b.x < a.x + a.width
        && a.y < b.y + b.height
        && b.y < a.y + a.height
}

fn point_in_any_room(rooms: &[aether_package::RoomDef], x: f32, y: f32) -> bool {
    rooms
        .iter()
        .any(|r| point_in_bounds(&r.bounds, x, y))
}

fn point_in_bounds(b: &RoomBoundsDef, x: f32, y: f32) -> bool {
    x >= b.x && x <= b.x + b.width && y >= b.y && y <= b.y + b.height
}

pub fn print_report(report: &PlaytestReport, json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(report).expect("json"));
        return;
    }
    let status = if report.ok { "OK" } else { "FAIL" };
    println!(
        "playtest {status}: {} scene={} rooms={} entities={} tiles={} collectibles={}",
        report.game_id,
        report.scene_id,
        report.room_count,
        report.entity_count,
        report.tile_layer_count,
        report.collectible_count,
    );
    for n in &report.notes {
        println!("  note: {n}");
    }
    for i in &report.issues {
        println!("  issue: {i}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_package::{
        CollectibleDef, EntityComponents, EntityDef, PackageMeta, RoomDef, SceneDef,
        TransformDef, WinConditionDef,
    };

    fn minimal_pkg() -> GamePackage {
        GamePackage {
            meta: PackageMeta {
                id: "t".into(),
                name: "t".into(),
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
                        transform: Some(TransformDef { x: 0.0, y: 0.0 }),
                        player: Some(aether_package::PlayerDef { speed: 220.0 }),
                        ..Default::default()
                    },
                },
                EntityDef {
                    id: "win".into(),
                    scene: "main".into(),
                    components: EntityComponents {
                        win_condition: Some(WinConditionDef {
                            requires_collectible: "shard".into(),
                            requires_all: Vec::new(),
                        }),
                        ..Default::default()
                    },
                },
                EntityDef {
                    id: "pickup_shard".into(),
                    scene: "main".into(),
                    components: EntityComponents {
                        transform: Some(TransformDef { x: 10.0, y: 10.0 }),
                        collectible: Some(CollectibleDef { id: "shard".into() }),
                        ..Default::default()
                    },
                },
            ],
            rooms: vec![RoomDef {
                id: "grove".into(),
                scene: "main".into(),
                bounds: RoomBoundsDef {
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 100.0,
                },
                label: None,
            }],
            mechanics: vec![],
            content: vec![],
            assets: vec![],
            tile_layers: vec![],
        }
    }

    #[test]
    fn detects_overlapping_rooms() {
        let mut pkg = minimal_pkg();
        pkg.rooms.push(RoomDef {
            id: "north".into(),
            scene: "main".into(),
            bounds: RoomBoundsDef {
                x: 50.0,
                y: 50.0,
                width: 100.0,
                height: 100.0,
            },
            label: None,
        });
        let report = playtest_package(&pkg);
        assert!(!report.ok);
        assert!(report.issues.iter().any(|i| i.contains("overlap")));
    }
}
