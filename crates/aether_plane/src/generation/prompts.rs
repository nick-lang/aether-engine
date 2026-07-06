//! Shared prompts for patch-generation providers.

use aether_package::GamePackage;

pub const PATCH_SYSTEM_PROMPT: &str = r#"You are a game world patch generator for Aether Engine.
Output ONLY valid JSON for a PatchDocument. No markdown, no explanation.

Schema:
{
  "patch_id": "unique_snake_case_id",
  "base_version": <package version number from user>,
  "ops": [
    { "op": "upsert_entity", "scene": "<scene_id>", "entity": { "id": "...", "scene": "...", "components": { "transform": { "x": 0, "y": 0 }, "collectible": { "id": "..." } } } },
    { "op": "upsert_room", "scene": "<scene_id>", "room": { "id": "...", "scene": "...", "bounds": { "x": 0, "y": 0, "width": 100, "height": 80 }, "label": "optional" } }
  ]
}

Rules:
- Use only upsert_entity and upsert_room ops.
- Entity must have player, collectible, win_condition, and/or transform for props.
- Do NOT add a "sprite" component — the art worker generates PNGs and attaches them separately (patches with sprite are rejected).
- Do NOT upsert entities with id "player" or "win".
- Use new entity ids for new props; do not duplicate existing ids unless intentionally moving them.
- Keep |transform.x| and |transform.y| at most 400.
- Rooms need positive width and height.
- Match base_version exactly as given."#;

/// Full creative patch authoring (Dream mode) — model chooses ops, bounds, and themes.
pub const DREAM_SYSTEM_PROMPT: &str = r#"You are a creative world builder for Aether Engine (top-down exploration).
Output ONLY one PatchDocument JSON object. No markdown, no commentary.

{
  "patch_id": "unique_snake_case_id",
  "base_version": <exact number from user>,
  "ops": [
    { "op": "upsert_room", "scene": "<scene_id>", "room": { "id": "...", "scene": "...", "bounds": { "x": 0, "y": 0, "width": 120, "height": 100 }, "label": "evocative name" } },
    { "op": "upsert_entity", "scene": "<scene_id>", "entity": { "id": "...", "scene": "...", "components": { "transform": { "x": 0, "y": 0 }, "collectible": { "id": "unique_collectible_id" } } } },
    { "op": "paint_tiles", "scene": "<scene_id>", "room_id": "...", "tile_size": 16, "fill": "moss|grass|floor|wall" }
  ]
}

Creative mandate:
- Interpret the creator's mood, story, and spatial vision freely (ruins, groves, flooded halls, crystal caves, etc.).
- Combine multiple ops in one patch when the prompt is expansive: several rooms, props, and floor styles that fit a coherent theme.
- Use evocative ids and labels (e.g. sunken_archive, ember_shard, moss_chapel).
- Place new rooms so they connect: e.g. a north room's y should meet the parent room's north edge (parent.y + parent.height).
- Align bounds to an 8px grid; use tile_size 16 for paint_tiles.
- Use paint_tiles with "fill" only — never enumerate hundreds of cell coordinates.

Hard rejections:
- No "sprite" components (art is generated separately).
- Do not upsert entities with id "player" or "win".
- |transform.x| and |transform.y| must be ≤ 400.
- All new entity and collectible ids must be unique vs the world summary.
- base_version must match the user message exactly."#;

/// System prompt for hybrid plan → Rust patch builder (small JSON only).
pub const PLAN_SYSTEM_PROMPT: &str = r#"You are an incremental game-world planner for Aether Engine.
Output ONLY valid JSON matching this schema. No markdown, no explanation.

{
  "actions": [
    { "type": "add_room", "direction": "north|south|east|west|center", "label": "optional string" },
    { "type": "add_collectible", "direction": "north|south|east|west|center", "id": "optional_snake_id", "label": "optional" },
    { "type": "paint_tiles", "direction": "north|south|east|west|center", "tile": "moss|grass|floor|wall", "room_id": "optional existing room id" }
  ]
}

Rules:
- Use only add_room, add_collectible, and paint_tiles.
- Pick direction from the creator's words (north/south/east/west). Default center if unclear.
- paint_tiles: floor a room (moss, grass, floor, wall). Use room_id when the prompt names a room; otherwise direction picks the target room. Do NOT list individual tile cells.
- For "mossy floor" / "grass floor" use paint_tiles only — do not add_room unless the prompt asks for a new room.
- Do NOT include sprites, player, win, or full PatchDocument — only this plan.
- One or two actions per response unless the prompt clearly asks for more.
- For add_collectible, optional id like golden_shard; omit id to auto-name."#;

pub fn dream_user_message(
    package_version: u64,
    scene_id: &str,
    prompt: &str,
    canonical_summary: Option<&str>,
) -> String {
    let mut out = format!(
        "package_version: {package_version}\nscene_id: {scene_id}\n\n\
         Creator vision (realize this as one cohesive patch):\n{prompt}"
    );
    if let Some(summary) = canonical_summary {
        out.push_str("\n\nExisting world (extend or reshape around this; do not duplicate ids):\n");
        out.push_str(summary);
    }
    out
}

pub fn plan_user_message(
    package_version: u64,
    scene_id: &str,
    prompt: &str,
    canonical_summary: Option<&str>,
) -> String {
    let mut out = format!(
        "package_version: {package_version}\nscene_id: {scene_id}\n\nCreator prompt:\n{prompt}"
    );
    if let Some(summary) = canonical_summary {
        out.push_str("\n\nCurrent world (avoid duplicate ids):\n");
        out.push_str(summary);
    }
    out
}

pub fn patch_user_message(
    package_version: u64,
    scene_id: &str,
    prompt: &str,
    canonical_summary: Option<&str>,
) -> String {
    let mut out = format!(
        "package_version: {package_version}\nscene_id: {scene_id}\n\nCreator prompt:\n{prompt}"
    );
    if let Some(summary) = canonical_summary {
        out.push_str("\n\nCurrent canonical world (do not duplicate entities; prefer new ids):\n");
        out.push_str(summary);
    }
    out
}

/// Compact snapshot of the live package for LLM context.
pub fn canonical_summary(package: &GamePackage) -> String {
    let scene_id = package.default_scene_id().unwrap_or("main");
    let mut lines = vec![format!(
        "meta: {} v{}",
        package.meta.name, package.meta.version
    )];
    for room in package.rooms_for_scene(scene_id) {
        let b = &room.bounds;
        lines.push(format!(
            "room {}: bounds x={} y={} w={} h={}",
            room.id, b.x, b.y, b.width, b.height
        ));
    }
    for layer in package.tile_layers_for_scene(scene_id) {
        let tile = layer
            .fill
            .as_deref()
            .or_else(|| layer.cells.first().map(|c| c.tile.as_str()))
            .unwrap_or("?");
        lines.push(format!(
            "tiles {}: fill={} ({} cells)",
            layer.room_id,
            tile,
            layer.cells.len()
        ));
    }
    for entity in package.entities_for_scene(scene_id) {
        let mut parts = vec![entity.id.clone()];
        if let Some(t) = &entity.components.transform {
            parts.push(format!("@({},{})", t.x, t.y));
        }
        if entity.components.player.is_some() {
            parts.push("player".into());
        }
        if let Some(c) = &entity.components.collectible {
            parts.push(format!("collectible:{}", c.id));
        }
        if entity.components.win_condition.is_some() {
            parts.push("win".into());
        }
        if let Some(s) = &entity.components.sprite {
            parts.push(format!("sprite:{}", s.asset));
        }
        lines.push(parts.join(" "));
    }
    lines.join("\n")
}

pub fn art_prompt_suffix(user_prompt: &str) -> String {
    format!(
        "pixel art game sprite, top-down view, simple, flat colors, centered, {user_prompt}"
    )
}
