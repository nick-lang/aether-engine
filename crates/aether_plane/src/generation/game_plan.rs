//! Parse natural-language prompts into a deterministic [`GamePlan`].

use aether_package::{
    attach_room, default_attach_height, default_attach_width, default_main_room_bounds,
    place_in_room, Direction, RoomAnchor, RoomBoundsDef,
};

/// Layout spec for a generated playable game.
#[derive(Debug, Clone, PartialEq)]
pub struct GamePlan {
    pub title: String,
    pub scene_id: String,
    pub scene_name: String,
    pub main_room_label: String,
    pub extra_rooms: Vec<RoomPlan>,
    pub collectibles: Vec<CollectiblePlan>,
    /// Collectible id that triggers the win condition (legacy single-id field).
    pub win_collectible_id: String,
    /// When true, win requires every collectible in `collectibles`.
    pub win_requires_all: bool,
    pub wants_hero_art: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoomPlan {
    pub id: String,
    pub label: String,
    pub bounds: RoomBounds,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CollectiblePlan {
    pub id: String,
    pub x: f32,
    pub y: f32,
}

/// True when the prompt describes a full game, not a single incremental patch.
pub fn prompt_is_full_game(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    [
        "generate a game",
        "generate game",
        "new game",
        "from scratch",
        "treasure hunt",
        "dungeon crawl",
        "dungeon crawler",
        "make a game",
        "build a game",
        "create a game",
        "minimal explorer",
        "collect the",
        "collect all",
        "find the",
        "win by collecting",
        "explorer game",
        "playable game",
    ]
    .iter()
    .any(|k| lower.contains(k))
}

pub fn parse_game_prompt(prompt: &str) -> GamePlan {
    let lower = prompt.to_lowercase();
    let title = infer_title(&lower);
    let shard_count = infer_collectible_count(&lower);
    let directions = infer_directions(&lower);
    let wants_hero_art = super::world::prompt_wants_art(prompt)
        || lower.contains("goblin")
        || lower.contains("hero")
        || lower.contains("character");

    let main_room_label = if lower.contains("dungeon") {
        "Dungeon Hall".into()
    } else if lower.contains("grove") || lower.contains("forest") {
        "Starting Grove".into()
    } else {
        "Starting Area".into()
    };

    let collectibles = build_collectibles(shard_count, &directions, &lower);
    let win_collectible_id = collectibles
        .first()
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "golden_shard".into());
    let win_requires_all = shard_count > 1
        || lower.contains("collect all")
        || lower.contains("all shard")
        || lower.contains("all key");

    let mut extra_rooms = Vec::new();
    for dir in &directions {
        if let Some(room) = room_for_direction(dir) {
            if !extra_rooms.iter().any(|r: &RoomPlan| r.id == room.id) {
                extra_rooms.push(room);
            }
        }
    }

    if extra_rooms.is_empty() && shard_count > 1 {
        extra_rooms.push(room_for_direction("east").unwrap());
        if shard_count > 2 {
            extra_rooms.push(room_for_direction("west").unwrap());
        }
    }

    GamePlan {
        title,
        scene_id: "main".into(),
        scene_name: "Main".into(),
        main_room_label,
        extra_rooms,
        collectibles,
        win_collectible_id,
        win_requires_all,
        wants_hero_art,
    }
}

fn infer_title(lower: &str) -> String {
    if lower.contains("dungeon") {
        "Dungeon Crawler".into()
    } else if lower.contains("grove") || lower.contains("forest") {
        "Grove Treasure Hunt".into()
    } else if lower.contains("goblin") {
        "Goblin Grove".into()
    } else {
        "Generated Explorer".into()
    }
}

fn infer_collectible_count(lower: &str) -> usize {
    if lower.contains("three") || lower.contains("3 shard") || lower.contains("3 key") {
        3
    } else if lower.contains("two") || lower.contains("2 shard") || lower.contains("2 key") {
        2
    } else {
        1
    }
}

fn infer_directions(lower: &str) -> Vec<&'static str> {
    let mut dirs = Vec::new();
    for d in ["north", "south", "east", "west"] {
        if lower.contains(d) {
            dirs.push(d);
        }
    }
    dirs
}

fn build_collectibles(count: usize, directions: &[&str], lower: &str) -> Vec<CollectiblePlan> {
    let ids: Vec<&str> = match count {
        3 => vec!["golden_shard", "silver_shard", "bronze_shard"],
        2 => vec!["golden_shard", "silver_shard"],
        _ => vec!["golden_shard"],
    };

    let mut out = Vec::new();
    for (i, id) in ids.iter().enumerate() {
        let dir = directions.get(i).copied();
        let (x, y) = collectible_position(dir, i, lower);
        out.push(CollectiblePlan {
            id: (*id).into(),
            x,
            y,
        });
    }
    out
}

fn starting_area_bounds() -> RoomBoundsDef {
    default_main_room_bounds()
}

fn collectible_position(direction: Option<&str>, index: usize, lower: &str) -> (f32, f32) {
    let main = starting_area_bounds();
    if let Some(dir) = direction {
        if let Some(d) = Direction::parse(dir) {
            let room = attach_room(
                &main,
                d,
                default_attach_width(d),
                default_attach_height(d),
            );
            return place_in_room(&room, RoomAnchor::for_direction(d));
        }
    }
    if lower.contains("golden") && index == 0 {
        return place_in_room(&main, RoomAnchor::East);
    }
    match index {
        0 => place_in_room(&main, RoomAnchor::East),
        1 => place_in_room(&main, RoomAnchor::West),
        _ => place_in_room(&main, RoomAnchor::South),
    }
}

fn room_for_direction(dir: &str) -> Option<RoomPlan> {
    let d = Direction::parse(dir)?;
    let (id, label) = match d {
        Direction::North => ("room_north", "Northern Grove"),
        Direction::South => ("room_south", "Southern Grove"),
        Direction::East => ("room_east", "Eastern Grove"),
        Direction::West => ("room_west", "Western Grove"),
    };
    let main = starting_area_bounds();
    let bounds = attach_room(
        &main,
        d,
        default_attach_width(d),
        default_attach_height(d),
    );
    Some(RoomPlan {
        id: id.into(),
        label: label.into(),
        bounds: RoomBounds {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_full_game_prompts() {
        assert!(prompt_is_full_game(
            "Generate a treasure hunt game with a shard in the north"
        ));
        assert!(!prompt_is_full_game("add a room to the north"));
    }

    #[test]
    fn builds_two_shard_plan() {
        let plan = parse_game_prompt("A game with two shards east and west");
        assert_eq!(plan.collectibles.len(), 2);
        assert!(plan.extra_rooms.len() >= 1);
        assert!(plan.win_requires_all);
    }

    #[test]
    fn collect_all_phrase_sets_requires_all() {
        let plan = parse_game_prompt("collect all three shards in the grove");
        assert_eq!(plan.collectibles.len(), 3);
        assert!(plan.win_requires_all);
    }
}
