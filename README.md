# Aether Engine

Custom AI-native game engine: data-defined worlds and mechanics, live canonical patches, and an AI content plane.

## Status

**Phase 1–2** complete — package runtime, content plane, Studio, live SSE patches.  
**Phase 3** in progress — simulated AI; real LLM + spatial/art next.

**Planning:** [Near term (actionable)](.cursor/plans/aether_engine_near_term.plan.md) · [Grand design (vision)](.cursor/plans/aether_engine_grand_design.plan.md) · [Agent guide](AGENTS.md)

## Quick start

Requires [Rust](https://rustup.rs/) on your PATH. Run from the repo root.

### Daily dev (plane + player in one step)

```powershell
cargo run -p aether_cli -- dev
```

On Windows, if the content plane is not running, `dev` opens a **new terminal** with `aether_plane`, waits for it, seeds `minimal_explorer` if needed, then starts the game with `--plane`.

```powershell
# Optional wrapper
.\scripts\dev.ps1
.\scripts\dev.ps1 -Watch
```

| Flag | Meaning |
|------|---------|
| `--watch` | Hot-reload `package.json` while playing |
| `--no-spawn-plane` | Error if plane is down (don't open a new window) |
| `--plane URL` | Default `http://127.0.0.1:8787` |
| `--skip-seed` | Skip seed step |
| `--no-open-studio` | Don't launch the browser |

Opens **Studio** in your default browser automatically (`http://127.0.0.1:8787/studio/`) once the plane is ready.

### Player only (no content plane)

```bash
cargo run -p aether_player
```

Controls: **WASD** / arrows. Collect the golden shard. **P** = local patch file demo.

## Manual setup (three terminals)

If you prefer separate terminals:

```bash
cargo run -p aether_plane
cargo run -p aether_plane -- seed minimal_explorer examples/minimal_explorer/package.json
cargo run -p aether_player -- --plane http://127.0.0.1:8787 examples/minimal_explorer/package.json
```

**Browser — [Aether Studio](http://127.0.0.1:8787/studio/)**

Submit a patch candidate → **Publish** → the running game updates live.

Or via CLI:

```bash
cargo run -p aether_cli -- plane submit minimal_explorer examples/minimal_explorer/patches/add_second_shard.json
cargo run -p aether_cli -- plane publish minimal_explorer add_second_shard
```

## Repository layout

- `crates/` — engine runtime + `aether_plane` service
- `studio/` — Aether Studio static UI
- `tools/` — CLI
- `docs/` — specs and guides
- `examples/` — sample packages and patches

## Docs

- [Engine constitution](docs/engine-constitution.md)
- [Game package format](docs/package-format.md)
- [Live patch protocol](docs/patch-protocol.md)
- [Content plane](docs/content-plane.md)
- [AI authoring](docs/ai-authoring.md)
- [Plan index](.cursor/plans/aether_engine_roadmap.plan.md)
- [Near-term plan](.cursor/plans/aether_engine_near_term.plan.md)
- [Grand design](.cursor/plans/aether_engine_grand_design.plan.md)
- [Agent guide (working with AI)](AGENTS.md)
