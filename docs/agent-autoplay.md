# Agent autoplay — test without the user

Coding agents should **drive the content plane over HTTP** and use **headless playtest**, not rely on opening Studio or the Bevy window every iteration.

## Continuous iteration

When the user wants unsupervised progress (“keep iterating until I pause”):

1. Follow `.cursor/skills/aether-autonomous-iterate/SKILL.md`
2. Maintain `.cursor/plans/aether_autonomous_queue.plan.md`
3. Log completed work in `.cursor/plans/aether_autonomous_session.md`
4. Stop only if `.cursor/PAUSE_AUTONOMOUS` exists or the user says pause

## Golden loop (every feature / fix)

```powershell
# 1. Ensure plane is up (spawns background process if needed)
cargo run -p aether_cli -- plane ensure --spawn

# 2. Baseline world health
cargo run -p aether_cli -- plane playtest minimal_explorer

# 3. Generate + publish (Dream for creative, simulated for fast CI)
cargo run -p aether_cli -- plane generate minimal_explorer "your prompt" --mode dream --json

# 4. Verify canonical
cargo run -p aether_cli -- plane playtest minimal_explorer --json

# 5. Unit tests
cargo test -p aether_plane -p aether_package --quiet
```

One-shot regression:

```powershell
cargo run -p aether_cli -- plane smoke minimal_explorer --mode simulated
```

## What agents can and cannot do

| Action | How |
|--------|-----|
| Generate patch | `plane generate` → POST `/packages/{id}/generate` |
| Publish | `auto_submit: true` on generate, or `plane publish` |
| Rollback | `plane rollback` |
| Read world | `plane canonical --json` |
| Inbox | `plane candidates --status pending` |
| Validate world | `plane playtest` (no GPU) |
| Ollama busy? | `GET /health/ollama/activity` via plane |

| Action | Not for agents by default |
|--------|---------------------------|
| Click Studio UI | Use HTTP instead |
| Play Bevy window | User verifies visuals; agent uses playtest |
| Commit / push | Only when user asks |

## Environment

- Copy `.env.example` → `.env`
- `AETHER_PLANE_URL=http://127.0.0.1:8787`
- Dream / Ollama: `OLLAMA_TIMEOUT_SECS=300`

## When the user should open the game

- Camera feel, tile art, pickup juice
- After agent reports `playtest OK` and tests pass

## Extending playtest

Add checks in `tools/aether_cli/src/playtest.rs` (room overlap, win reachable, etc.) so agents catch more without manual play.
