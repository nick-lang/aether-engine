# Aether Content Plane (Phase 2)

The content plane governs canonical packages and published patches. Runtimes subscribe via SSE; Studio reviews and publishes candidates.

## Run the plane

```bash
cargo run -p aether_plane
# Studio: http://127.0.0.1:8787/studio/
```

Seed a game from a local package:

```bash
cargo run -p aether_plane -- seed minimal_explorer examples/minimal_explorer/package.json
```

## API (v0)

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/health` | Liveness |
| GET | `/packages/{id}/manifest` | Package version, event sequence, `active_patches` |
| GET | `/packages/{id}/canonical` | Canonical `GamePackage` JSON |
| GET | `/packages/{id}/patches?since=N` | Published patches after sequence N |
| GET | `/packages/{id}/candidates` | Candidates (`?status=pending` for inbox only) |
| GET | `/packages/{id}/patches?since=N` | Published patch audit log |
| POST | `/packages/{id}/candidates` | Submit patch candidate |
| POST | `/packages/{id}/candidates/{patch_id}/publish` | Publish → SSE fanout |
| POST | `/packages/{id}/candidates/{patch_id}/reject` | Reject candidate |
| GET | `/packages/{id}/stream?since=N` | SSE `patch_published` events |
| POST | `/packages/{id}/events` | World Director gameplay events |
| POST | `/packages/{id}/generate` | AI patch generation (`simulated` or `openai`) |
| POST | `/packages/{id}/generate-art` | Art stub → PNG + sprite patch candidate |
| POST | `/packages/{id}/rollback` | Remove patch from `active_patches` and refold; cascades dependent patches (e.g. moss tiles when room is rolled back); optional `{ "patch_id": "..." }` |
| PATCH | `/packages/{id}/patches/{patch_id}/label` | `{ "label": "North grove v1" }` — display name; `patch_id` stays stable for re-apply |
| POST | `/packages/{id}/patches/{patch_id}/reactivate` | Re-apply a rolled-back patch from audit history |
| DELETE | `/packages/{id}/patches/{patch_id}` | Remove inactive patch audit + archived candidate; prune orphan PNGs |
| POST | `/packages/{id}/prune` | Workspace cleanup (orphan assets, optional history/archive wipe) |
| GET | `/packages/{id}/assets/{asset_id}` | PNG asset bytes |

Data is stored under `data/plane/{game_id}/` by default.

| Folder | What it is | Rollback touches? |
|--------|------------|-------------------|
| `canonical/package.json` | Live world (refolded) | Rebuilt from base + active patches |
| `canonical/base.json` | Seed / generate-game baseline | Updated on seed, generate-game, history wipe |
| `published/` | Studio **History** audit log | Append-only (rollback does not delete) |
| `candidates/archived/` | Every publish/generate inbox record | No |
| `assets/*.png` | Art worker output | Orphans pruned on rollback / **Prune** |

**Rollback** removes a `patch_id` from `active_patches` (omit body or `{}` for last applied) and refolds canonical from `base.json`. Patches that no longer fit the world (e.g. `paint_tiles` on a removed room) are removed from the active set automatically. Tile layers without a matching room are stripped on refold. History JSON stays for audit unless you **Delete** an inactive patch. `published_sequence` only increases on publish (safe for SSE). Studio History: **Rollback** (active), **Re-apply** / **Delete** (rolled back), **Rename** (label in `manifest.patch_labels`).

```http
POST /packages/{id}/prune
{ "orphan_assets": true }

POST /packages/{id}/prune
{
  "orphan_assets": true,
  "clear_publish_history": true,
  "clear_archived_candidates": true,
  "clear_pending_candidates": true
}
```

Studio: **Prune assets** (orphan PNGs only) or **Clean workspace** (full audit cleanup, keeps canonical).

## Resetting stale plane data

The player connects with `?since=<published_sequence>` and replays any newer publishes. If `manifest.json` is out of sync with `published/` (or you re-seeded without clearing history), you may see old shards/rooms on startup.

```bash
cargo run -p aether_cli -- plane reset minimal_explorer
cargo run -p aether_plane -- seed minimal_explorer examples/minimal_explorer/package.json
```

`seed` now wipes the game folder before writing a fresh canonical package.

## Live session workflow

**One command (recommended):**

```bash
cargo run -p aether_cli -- dev
```

**Manual — plane, seed, player:**

```bash
cargo run -p aether_plane
cargo run -p aether_plane -- seed minimal_explorer examples/minimal_explorer/package.json
cargo run -p aether_player -- --plane http://127.0.0.1:8787 examples/minimal_explorer/package.json
```

**Browser — Studio**

Open http://127.0.0.1:8787/studio/ → submit candidate → **Publish**. The running player applies the patch without restart.

The player camera follows the player (world stays in absolute coordinates; packages use `aether_package::layout` for aligned rooms and prop placement).

**CLI alternative**

```bash
cargo run -p aether_cli -- plane submit minimal_explorer examples/minimal_explorer/patches/add_second_shard.json
cargo run -p aether_cli -- plane publish minimal_explorer add_second_shard
```

## World Director (stub)

POST a gameplay event to auto-queue a candidate patch:

```json
POST /packages/minimal_explorer/events
{ "event_type": "spawn_bonus_shard", "payload": {} }
```

Then publish the new candidate from Studio.
