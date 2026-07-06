# AI authoring (Phase 3)

Phase 3 adds **generation workers** that produce structured patch JSON, validate it, and queue it as candidates on the content plane.

## Canonical package

Publishing merges ops into `data/plane/{game_id}/canonical/package.json`. With `--plane`, the player loads that canonical JSON at startup (not just local `package.json`).

## Simulated worker (default)

No API key required. Maps natural-language prompts to valid patches.

**Shard / collectible prompts:**

- Mention **shard**, **collectible**, or **treasure**
- Optional direction: **north**, **south**, **east**, **west**

Example: `Add a golden shard to the north of the grove`

**Room prompts:**

- Mention **room**
- Optional direction for placement

Example: `Add a room to the north`

**Floor / tile prompts:**

- Keywords: **moss**, **grass**, **floor**, **tiles**, often with a direction or room
- Produces `paint_tiles` with `"fill": "moss"` (not a per-cell list); the player tessellates inside the room bounds with edge clipping

Example: `Mossy floor in the north room`

New rooms from **add room** plans also get an automatic floor fill (moss for north, grass elsewhere). **Generate game** seeds grass on the main area and moss on northern extra rooms.

Ollama **plan** mode understands `{ "type": "paint_tiles", "tile": "moss", "direction": "north" }` — see `PLAN_SYSTEM_PROMPT` in `prompts.rs`.

### Dream mode (open-ended authoring)

Studio **Dream (Ollama full patch)** or API `"mode": "dream"` sends a creative system prompt; the model outputs a full `PatchDocument` with rooms, entities, and `paint_tiles` fills. There is **no keyword fallback** and **no plan builder** — the model chooses bounds, labels, and themes. Rust only repairs (grid snap, compact paint) and validates.

Example prompt:

> A sunken moonlit library west of the grove, crystal floor tiles, two lore shards, and a mossy reading nook

Use `OLLAMA_TIMEOUT_SECS=300` for larger models. Expect some rejects; read the LLM trace and edit JSON before publish.

| Mode | Who authors the world? | Best for |
|------|------------------------|----------|
| `simulated` | Keywords + Rust | CI, fast demos |
| `ollama` | Small plan → Rust layout | Reliable incremental edits |
| `dream` | Model → full patch | Creative / open-ended prompts |
| `openai` | Model → full patch (legacy prompt) | Cloud |

## API

```http
POST /packages/{game_id}/generate
Content-Type: application/json

{
  "prompt": "add a shard to the east",
  "mode": "simulated",
  "auto_submit": true
}
```

`mode`: `simulated` | `ollama` | `dream` | `openai`

Response:

```json
{
  "patch": { "patch_id": "...", "base_version": 1, "ops": [...] },
  "provider": "simulated",
  "submitted": true
}
```

## Studio workflow

1. Open http://127.0.0.1:8787/studio/
2. Enter a prompt → **Generate candidate**
3. Review JSON → **Publish**
4. Running `aether_player --plane ...` applies the patch live

## CLI

```bash
cargo run -p aether_cli -- plane generate minimal_explorer "add a shard to the north"
cargo run -p aether_cli -- plane publish minimal_explorer <patch_id>
```

## Guardrails

Before a patch becomes a candidate:

- `base_version` must match the canonical manifest
- Only `upsert_entity` and `upsert_room` ops (v0)
- Max 32 ops per patch
- Non-empty `patch_id`

### LLM-only rules (`validate_llm_patch`)

When mode is **Ollama** or **OpenAI**, the plane also rejects:

- `sprite` on any entity (use **generate-art** / **generate-world** for pixels)
- Upserts to `player` or `win`
- Wrong `scene`, duplicate entity ids in one patch, coords outside ±400

Responses include a **`trace`** object: `raw_model_text`, `validation_notes`, optional `parse_error`. Studio **Requests** panel has an **LLM trace** expander. Traces are also **persisted** under `data/plane/{game_id}/traces/` (Studio **Saved traces**, CLI `plane traces <game_id>`, `GET /packages/{id}/traces`).

Submit/publish rejects **no-op** patches (canonical unchanged after apply).

**Incremental workflow (Phase A):** generate patch → expand trace → fix prompt if needed → publish. Goal: 10 reliable incremental prompts without hand-editing JSON (see near-term plan checklist).

## Repair pass

`generation::parse_model_output` strips markdown JSON fences for real LLM output (used when OpenAI mode is added).

## Ollama (local LLM, hybrid plan mode)

Ollama does **not** emit full patch JSON by default. It emits a small **plan**:

```json
{ "actions": [{ "type": "add_room", "direction": "north" }] }
```

Rust (`plan_builder`) turns that into a valid `PatchDocument` (coords, ids, no sprites). Faster and more reliable than `qwen3.6`-sized full-json generation.

1. Install [Ollama](https://ollama.com) and run `ollama serve`.
2. Pull a **small** chat model: `ollama pull llama3.2` (set `OLLAMA_MODEL=llama3.2` in `.env`).
3. In Studio, choose **Ollama (plan → patch)** for patch generate, or:

```json
{ "prompt": "add a room north of the grove", "mode": "ollama", "auto_submit": true }
```

Env (see `.env.example`): `OLLAMA_URL`, `OLLAMA_MODEL`, `OLLAMA_TIMEOUT_SECS`. Optional `OLLAMA_LEGACY_PATCH=1` restores slow full-patch mode.

Studio **LLM trace** shows the plan JSON returned by the model. If JSON is invalid, the plane falls back to keyword planning (same as simulated).

**Studio progress:** While a generate request is running, Studio polls `GET /health/ollama/activity` (Ollama `/api/ps`) every 2s and shows whether your model is loaded in VRAM / processing. This is not token-level streaming; for true % progress you would need async jobs + Ollama `stream: true` (future).

## OpenAI mode (cloud)

Set `OPENAI_API_KEY` (and optional `OPENAI_MODEL`) where `aether_plane` runs. Studio → **OpenAI**.

## Generate a full game (viability path)

`POST /packages/{id}/generate-game` builds a **complete playable package** from one prompt:

- Player, win condition, collectible(s), starting room (+ optional direction rooms)
- Validates against `GamePackage` rules before writing canonical
- **`apply_canonical: true`** (default) replaces the live world and notifies `--plane` players via SSE

```json
{
  "prompt": "Generate a treasure hunt game — collect the golden shard in the north grove",
  "patch_mode": "simulated",
  "art_mode": "automatic1111",
  "placement": "keywords",
  "include_art": true,
  "apply_canonical": true
}
```

Studio: **Generate & apply game** (top panel). Uses the simulated planner for reliability; Ollama is for incremental patches / placement, not full-game bootstrap yet.

After apply: run `cargo run -p aether_cli -- dev`, move with WASD, collect the shard, win screen triggers.

## Combined world + art

`POST /packages/{id}/generate-world` — one prompt, one candidate:

1. **Patch** via `patch_mode` (`simulated` | `ollama` | `openai`) — rooms, shards, etc.
2. **Art** when `include_art` is true, or omitted and the prompt matches visual keywords (sprite, chest, goblin, …).

```json
{
  "prompt": "Add a goblin camp to the east with a goblin sprite",
  "patch_mode": "ollama",
  "art_mode": "automatic1111",
  "placement": "auto",
  "include_art": null,
  "auto_submit": true
}
```

`include_art`: `true` | `false` | omit (auto). Studio **Generate world** uses this endpoint.

## Art generation

`POST /packages/{id}/generate-art` with `mode`:

| mode | Requires |
|------|----------|
| `stub` | Nothing (hash-colored 64×64) |
| `ollama` | Ollama + image model (`ollama pull flux`, `OLLAMA_IMAGE_MODEL`) |
| `automatic1111` | [AUTOMATIC1111 WebUI](https://github.com/AUTOMATIC1111/stable-diffusion-webui) with `--api`, `A1111_URL` |

Studio has an art mode dropdown. Publish to show a sprite on `prop_sprite` (player needs `--plane`).

**Placement** (separate from the image backend):

| `placement` | Behavior |
|-------------|----------|
| `auto` (default) | Ollama JSON `{x,y}` when `OLLAMA_URL` / `OLLAMA_MODEL` set; else keyword heuristics |
| `ollama` | Always ask Ollama; fall back to keywords on failure |
| `keywords` | `north` / `east` / … → fixed offsets (legacy demo) |

A1111 only draws the PNG. The plane builds patch JSON (entity + `transform` + `sprite`); Ollama is optional for coordinates, not for pixels when using `automatic1111`.

## Rollback

`POST /packages/{id}/rollback` removes a patch from `active_patches` and refolds the world from `canonical/base.json`. Body: `{ "patch_id": "add_north_room" }` or empty for last applied. Audit files in `published/` are kept. A running `aether_player --plane` session reloads via SSE `canonical_reset`.

## Next

- Mechanic and content patch ops
- Stronger rule engine (economy, anti-farm)
