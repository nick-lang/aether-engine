---
name: Aether Engine — Plan index
overview: Index of planning documents. Use near-term for implementation; grand design for vision; this file tracks phase completion at a glance.
todos:
  - id: phase1-kernel
    content: Runtime kernel — GamePackage, Bevy player, hot reload
    status: completed
  - id: phase2-plane
    content: Content plane, Studio, SSE live patches
    status: completed
  - id: phase3-ai
    content: AI authoring — plan builder, traces, rollback, generate-game
    status: completed
  - id: phase3b-layout
    content: Layout context for drafts + paint_tiles / tile render (near-term plan)
    status: in_progress
  - id: phase4-spatial-art
    content: Rooms/tiles polish, asset pipeline, spatial Studio preview
    status: pending
  - id: phase5-editor
    content: In-engine editor tooling
    status: pending
  - id: phase6-multiplayer
    content: Session gateway, patch fanout
    status: pending
isProject: false
---

# Aether Engine — Plan index

Planning is split so you can work efficiently with the agent:

| Plan | Use when |
|------|----------|
| **[Near term (actionable)](aether_engine_near_term.plan.md)** | “What should we build this week?” — **set as active project plan** |
| **[Grand design (reference)](aether_engine_grand_design.plan.md)** | “Does this fit the vision?” — world authoring, art, Cursor-for-worlds |
| **This index** | Phase completion checklist |

## Phase checklist

| Phase | Status | Notes |
|-------|--------|-------|
| 0 Genesis | ✓ | Workspace, docs, crates |
| 1 Runtime | ✓ | `aether_player`, minimal_explorer |
| 2 Live plane | ✓ | Plane, Studio, `--plane` |
| 3 AI authoring | ✓ | Plan mode, Studio traces, active_patches, generate-game |
| 3b Layout + tiles | In progress | Camera + room chain shipped; layout context + paint_tiles next |
| 4 Spatial + art | Planned | Tile polish, PNG pipeline, Studio spatial preview |
| 5 Editor | Planned | egui / in-engine tools |
| 6 Multiplayer | Planned | Gateway, fanout |

## Architecture (unchanged)

Runtime (`aether-engine` repo) ↔ Content plane (`aether_plane`) ↔ Studio (`/studio/`).

See grand design for diagrams and long-term ontology.
