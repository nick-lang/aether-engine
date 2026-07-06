# Agent guide — Aether Engine

How to work on this repo with Cursor (or other coding agents).

## Which plan to read

| Question | Read |
|----------|------|
| What do I implement today? | `.cursor/plans/aether_engine_near_term.plan.md` |
| Does this fit the long-term vision? | `.cursor/plans/aether_engine_grand_design.plan.md` |
| What phase are we in? | `.cursor/plans/aether_engine_roadmap.plan.md` (index) |

**Default:** Follow the **near-term** plan unless the user explicitly asks for vision-only discussion.

## Repo map

| Path | What |
|------|------|
| `crates/aether_player` | Bevy game binary — **only this opens a window** |
| `crates/aether_plane` | HTTP + SSE content plane |
| `crates/aether_package` | GamePackage load/validate |
| `crates/aether_patch` | Patch types |
| `crates/aether_ecs` | Spawn entities from package |
| `studio/` | Static Studio UI |
| `examples/minimal_explorer/` | Reference game package + patches |
| `docs/` | Specs (constitution, formats, plane, AI) |

## Conventions

- **Minimize scope** — one near-term item per task when possible.
- **No engine forks per game** — extend `GamePackage`, patch ops, and plane workers.
- **Validate before publish** — use `aether_package::validate_package` and plane rules.
- **Plane vs player** — generation and canonicalization live on the plane; runtime applies patches only.
- **2D first** — do not add 3D/MMO unless the near-term plan says so.
- **Do not commit** unless the user asks.

## Environment variables

Copy `.env.example` to `.env` (gitignored) when using real LLM or custom paths:

| Variable | Purpose |
|----------|---------|
| `OPENAI_API_KEY` | Real patch generation (`mode: openai`) — optional; simulated works without |
| `AETHER_PLANE_DATA` | Canonical store root (default `data/plane`) |
| `AETHER_PLANE_PORT` | Plane HTTP port (default `8787`) |
| `AETHER_PLANE_URL` | Used by `aether_cli` plane subcommands (default `http://127.0.0.1:8787`) |

## Typical dev session

**Preferred (one command):**

```powershell
cargo run -p aether_cli -- dev
# or: .\scripts\dev.ps1
```

**Manual (three terminals):**

```powershell
cargo run -p aether_plane
cargo run -p aether_plane -- seed minimal_explorer examples/minimal_explorer/package.json
cargo run -p aether_player -- --plane http://127.0.0.1:8787 examples/minimal_explorer/package.json
```

Studio: http://127.0.0.1:8787/studio/

## Autonomous iteration (default when user says “keep going”)

**Continue without asking** until the user pauses or creates `.cursor/PAUSE_AUTONOMOUS`.

1. Read `.cursor/skills/aether-autonomous-iterate/SKILL.md` (queue + loop).
2. Work from `.cursor/plans/aether_autonomous_queue.plan.md` — pick top item, implement, verify, mark done, log session.
3. Read `.cursor/skills/aether-autoplay/SKILL.md` for CLI verification (`plane playtest`, `plane smoke`).
4. Add new todos to the queue when you find gaps; sync near-term plan when milestones complete.
5. **Do not commit** unless asked. **Do not open Bevy** unless visual QA is needed.

Pause: say “pause” or `New-Item .cursor/PAUSE_AUTONOMOUS` (see `.cursor/PAUSE_AUTONOMOUS.example`).

See `docs/agent-autoplay.md`.

## Suggested chat prompts

- “Implement the next pending item in `aether_engine_near_term.plan.md`.”
- “Check this approach against grand design and say if we should defer.”
- “Add API + Studio for X; update `docs/content-plane.md`.”
- “Only explain, don’t code” — for architecture questions.

## Tests

```powershell
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

## Updating plans

When finishing a near-term item, update todo status in `aether_engine_near_term.plan.md` and mention any grand-design assumptions that changed.
