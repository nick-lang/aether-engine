---
name: Aether — Near term (actionable)
overview: What to build in the next few weeks. Each item should ship a demo you can run locally. Check grand design for the north star; use this plan for day-to-day work with the agent.
todos:
  - id: phase-a-core
    content: "Phase A: plan builder, traces, validate_llm_patch, generate-game, active_patches rollback"
    status: completed
  - id: world-layout-runtime
    content: "World: camera follow, attach_room / extremal chain, pickup by entity id"
    status: completed
  - id: layout-context
    content: "Layout context for generate (canonical + pending inbox + in-patch virtual rooms)"
    status: completed
  - id: paint-tiles
    content: "Phase B: paint_tiles patch op + minimal 2D tile render in player"
    status: completed
  - id: llm-incremental-harness
    content: "Optional: run 10-prompt incremental checklist; log failure modes"
    status: completed
  - id: paint-tiles-generate
    content: "Phase B: Simulated keywords + plan PaintTiles; LLM plan schema for tiles (optional)"
    status: completed
  - id: connect-space-b2
    content: "B.2: auto floor fill on new rooms + game generate tile layers; checklist tests"
    status: completed
  - id: paint-tiles-fill
    content: "paint_tiles fill mode + edge-clipped tessellation at spawn (image UVs later)"
    status: completed
  - id: dream-mode
    content: "Dream mode: Ollama full creative PatchDocument (no plan/keyword fallback)"
    status: completed
  - id: nested-rooms-future
    content: "Future: parent_room + local bounds (interiors), not stacked world rects"
    status: pending
isProject: true
---

# Aether Engine — Near-term plan (actionable)

**Use this file** when you ask the agent to implement features.  
**Reference:** [Grand design](aether_engine_grand_design.plan.md) for long-term vision.

**Direction:** **A (authoring intelligence)** ✓ → **bridge (layout context)** → **B (world richness)**.

---

## Phase A — LLM incremental edits ✓ (shipped)

**Goal:** Ollama/OpenAI produce **publishable incremental patches** you trust without hand-editing JSON.

**Sign-off:** Manual testing OK (rollback, republish, camera, room chain on publish).

### Shipped

- `GenerationTrace` on `/generate` and `/generate-world`; Studio **LLM trace**
- `validate_llm_patch` (no sprite, core entities, bounds, duplicate collectible ids)
- **Hybrid Ollama:** `IncrementalPlan` → `plan_builder` → patch (`ollama_plan`); `llama3.2` default
- **Generate game:** `GamePlan` → `game_builder` → `replace_canonical_package`
- **`active_patches` rollback** (by `patch_id`, out-of-order); monotonic publish sequence; audit log kept
- **Prune / clean workspace**; Studio history rollback buttons
- **World layout:** `aether_package::layout` (`attach_room`, `extremal_room_bounds`, `place_in_room`)
- **Generation** uses layout from canonical (chained north rooms **after publish**)
- **Player:** camera follows player; movement not frozen on win; pickup tracks **entity id** (not only collectible id)

### Optional (not blocking B)

- [ ] Formal **10-prompt checklist** (below) with your Ollama model; note failures in issues
- [x] Persist traces under `data/plane/{id}/traces/`
- [x] Reject no-op / rename-only patches

### 10-prompt incremental checklist

Generate (Ollama) → publish each → verify in `--plane` player:

1. Shard east  
2. Shard north  
3. Room north (borders align with grove)  
4. Room west  
5. Collectible in southern room (after #3)  
6. Second shard with **new** entity + collectible id  
7. "sprite" in patch → validation fail  
8. Room east (unique id)  
9. Pickup far north (within bounds)  
10. Rollback one patch, republish shard  

---

## Bridge — Layout context (recommended next, ~1 session)

**Problem:** Two generates **before publish** (or two `add_room` in one plan) both read **canonical only** → same bounds → stacked overlays.

**Not the same as** “room inside room” (future: `parent_room` + local coords).

**Goal:** One **layout context** used by `spatial.rs` / `plan_builder`:

```text
layout = canonical rooms/entities
       + pending candidate upsert_room / upsert_entity
       + rooms already emitted in this patch (in-loop)
```

**Done when:** Generate north twice in inbox → second candidate at `y: 240`; single plan with two north rooms chains correctly.

**Keep:** World-space sibling `attach_room` for overworld expansion (works with camera, patches, rollback, future tiles).

---

## Phase B — Spatial richness (after bridge or in parallel if inbox OK)

**Goal:** Generated worlds **look** like places, not dots in a rectangle.

### B.1 `paint_tiles` ✓ (fill mode)

- `paint_tiles` with **`fill`** (uniform room) or legacy **`cells`** (sparse)  
- Tessellation at spawn with **edge-clipped** quads (flush to room bounds)  
- Simulated keyword: "mossy floor in the north room" → compact patch, no cell explosion  
- **Later:** tile PNGs + UV sub-rect on same `fill` contract

### B.2 Connect space ✓

- Tiles + room bounds + shard placement via layout context  
- **Generate game:** grass main + moss north (tile_layers on bootstrap)  
- **Add room (plan):** auto `paint_tiles` fill (moss north, grass other)  
- **Ollama plan:** `paint_tiles` action in schema; canonical summary lists floors  
- Simulated **checklist tests** in `incremental_checklist.rs` (Ollama manual: same prompts in Studio)  
- Optional later: collect-all win or second required pickup  

**Done when:** Generate game (or incremental publish) yields two areas you can walk between and tell apart visually.

---

## Future — Nested rooms (do not use stacked rects)

When scope is **interiors** (tavern cellar inside tavern):

- `RoomDef.parent_room: Option<String>` + **local** bounds  
- Runtime: world transform = parent chain  
- Separate from **sibling** `attach_room` on the overworld  

Until then, overlapping world rectangles = bug or duplicate draft, not nesting.

---

## Frozen demo path (do not regress)

```powershell
cargo run -p aether_cli -- dev
# Studio → Generate & apply game (stub art) → play → collect shards → explore after win
```

Full game = **simulated** planner. Incremental = **Ollama** + trace review.

---

## Quick commands

```powershell
cargo run -p aether_cli -- dev
# Studio: http://127.0.0.1:8787/studio/
cargo run -p aether_cli -- plane rollback minimal_explorer
cargo run -p aether_cli -- plane rollback minimal_explorer add_north_room
```

See [docs/ai-authoring.md](../../docs/ai-authoring.md), [docs/content-plane.md](../../docs/content-plane.md), `.env.example`.
