# Game Package Format (v0 draft)

A **GamePackage** is the unit of authorship: everything playable is defined as versioned data the runtime loads.

Ontology evolution (rooms, tiles, assets, mechanics) is described in the [grand design plan](../.cursor/plans/aether_engine_grand_design.plan.md#world-ontology-grows-over-time). Near-term work adds **`upsert_room`** before tile grids.

## Top-level sections

| Section | Purpose |
|---------|---------|
| `meta` | Package id, version, style profile |
| `scenes` | Spatial layout, spawn points, biome tags |
| `rooms` | Axis-aligned rectangular regions (2D bounds, rendered as outlines) |
| `entities` | Prefab-like definitions (component lists) |
| `mechanics` | Triggers, conditions, effects |
| `content` | Quests, NPCs, dungeons, rewards |
| `assets` | References and provenance metadata |

## Example (minimal)

See `examples/minimal_explorer/package.json`.

## Versioning

- `meta.version` is monotonic per package id.
- Patches reference `base_version` when applying live updates.
