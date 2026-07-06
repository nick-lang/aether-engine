---
name: aether-autoplay
description: Autonomous Aether iteration — drive the content plane via CLI, headless playtest, generate/publish/rollback without opening Studio or Bevy. Use when implementing features, fixing bugs, or validating Dream/plan generation without asking the user to click through UI.
---

# Aether autoplay

## Rules

1. **Do not ask the user to test** until playtest + `cargo test` pass (unless the task is explicitly visual).
2. **Start plane** with `cargo run -p aether_cli -- plane ensure --spawn` (repo root).
3. **Never skip playtest** after changing generation, patches, tiles, or plane store.
4. **Prefer HTTP/CLI** over Studio browser; Studio is for the user's review.
5. **Do not open Bevy** (`aether dev`) unless the user asked for visual QA.

## Commands (PowerShell, repo root)

```powershell
cargo run -p aether_cli -- plane ensure --spawn
cargo run -p aether_cli -- plane playtest minimal_explorer
cargo run -p aether_cli -- plane generate minimal_explorer "<prompt>" --mode dream --json
cargo run -p aether_cli -- plane playtest minimal_explorer --json
cargo run -p aether_cli -- plane smoke minimal_explorer --mode simulated
cargo test -p aether_plane -p aether_package -p aether_cli --quiet
```

Game id defaults to `minimal_explorer` unless the task says otherwise.

## Modes

| `--mode` | Use when |
|----------|----------|
| `simulated` | Fast regression, CI-style |
| `dream` | Creative / multi-room prompts (Ollama, no keyword fallback) |
| `ollama` | Structured incremental (plan → Rust) |

## If generate fails

- Read JSON `trace.parse_error` and `trace.validation_notes`.
- Fix Rust validation/prompts or repair layer — do not hand-edit `data/plane/` unless debugging.
- Retry with a narrower prompt before escalating to the user.

## Docs

- Full playbook: `docs/agent-autoplay.md`
- Agent map: `AGENTS.md`
- Near-term scope: `.cursor/plans/aether_engine_near_term.plan.md`
