---
name: Aether — Autonomous queue (agent-maintained)
overview: Living backlog for unsupervised agent iteration. Agents pick the top actionable item, implement, verify with playtest/tests, then update this file. Pause when user says stop or `.cursor/PAUSE_AUTONOMOUS` exists.
todos:
  - id: playtest-v2
    content: "Playtest: room overlap, duplicate ids, win collectible exists"
    status: completed
  - id: dream-retry
    content: "Dream: one automatic retry with validation errors in user message"
    status: completed
  - id: plan-respect-paint
    content: "Plan builder: skip auto-floor when plan already has paint_tiles for that room"
    status: completed
  - id: persist-traces
    content: "Persist generation traces under data/plane/{id}/traces/"
    status: completed
  - id: collect-all-win
    content: "Optional win_condition requires N collectibles (package + sim)"
    status: completed
  - id: tile-catalog-stub
    content: "Tile id → color map extensible for future PNG catalog"
    status: completed
  - id: reject-noop-patches
    content: "Reject no-op / rename-only patches at submit and publish"
    status: completed
  - id: studio-trace-history
    content: "Studio panel listing persisted traces from GET /traces"
    status: completed
  - id: sync-near-term-docs
    content: "Mark persist-traces done in near-term plan; update ai-authoring"
    status: completed
  - id: dream-overlap-repair
    content: "Dream: snap/repair overlapping room bounds before validate"
    status: completed
  - id: tile-png-catalog
    content: "Wire tile_asset_id to plane assets; UV sub-rect on fill tessellation"
    status: pending
  - id: ollama-checklist-cli
    content: "CLI runner for 10-prompt incremental checklist (log failures)"
    status: pending
isProject: true
---

# Autonomous work queue

**Protocol:** `.cursor/skills/aether-autonomous-iterate/SKILL.md`  
**Session log:** `.cursor/plans/aether_autonomous_session.md`  
**Pause:** create empty file `.cursor/PAUSE_AUTONOMOUS` or tell the agent to stop.

**Verify every item:** `cargo test -p aether_plane -p aether_package -p aether_cli` + `plane playtest minimal_explorer`

---

## Priority order

1. **playtest-v2** — agents catch broken worlds without human play  
2. **dream-retry** — fewer failed Dream publishes  
3. **plan-respect-paint** — less override of model intent in structured mode  
4. **persist-traces** — debug Ollama without Studio  
5. **collect-all-win** — richer game goals  
6. **tile-catalog-stub** — path to image tiles  

## Icebox (do not start without user)

- Nested rooms (`parent_room`)  
- Full tilemap / autotile  
- MMO / 3D  

## Gap discovery (agent adds rows here)

When playtest/smoke/Dream fails or code review reveals a hole, append a new todo above icebox with one-line scope.

- **reject-noop-patches** — near-term optional A  
- **studio-trace-history** — persisted traces without CLI  
- **sync-near-term-docs** — checklist housekeeping  
