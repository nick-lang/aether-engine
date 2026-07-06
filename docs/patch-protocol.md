# Live Patch Protocol (v0 draft)

Patches are small, typed, reversible updates applied to a running game session.

## Patch document

```json
{
  "patch_id": "patch_abc",
  "base_version": 12,
  "ops": [
    { "op": "upsert_entity", "scene": "main", "entity": {} },
    { "op": "upsert_room", "scene": "main", "room": { "id": "north", "scene": "main", "bounds": { "x": 0, "y": 100, "width": 200, "height": 80 } } },
    { "op": "upsert_mechanic", "mechanic": {} },
    { "op": "deactivate_content", "content_id": "quest_old" }
  ]
}
```

## Runtime rules

1. Reject patches when `base_version` does not match the active package manifest.
2. Apply all operations atomically at a simulation tick boundary.
3. Roll back and report `PatchRejected` if invariants fail after apply.
4. Emit `PatchApplied` to the content plane for audit.

## Transport (Phase 2)

- Manifest: `GET /packages/{game_id}/manifest`
- Deltas: `GET /patches?since={version}` or SSE/WebSocket push
