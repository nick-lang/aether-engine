---
name: aether-autonomous-iterate
description: Long-running autonomous Aether development. Maintain the queue plan, implement features, verify via CLI playtest, update plans, and continue to the next item until the user pauses. Use when the user wants the agent to keep building without constant input, or says "keep iterating", "autonomous mode", or "until I pause".
---

# Aether autonomous iteration

## When this applies

User wants you to **keep going** — implement, test, plan gaps, repeat — until they pause.

Also read: `.cursor/skills/aether-autoplay/SKILL.md` (CLI verification).

## Pause conditions (STOP and wait)

- User message says **pause**, **stop**, **hold**, or redirects to a different task  
- File exists: **`.cursor/PAUSE_AUTONOMOUS`** (repo root `.cursor/`)  
- Blocker needs secrets, product call, or destructive action user forbade  
- Same test fails **3 times** after fixes — log blocker in session, stop  

## Do NOT stop for

- Normal compile/test failures (fix and retry)  
- Dream/Ollama timeout (try simulated to validate Rust, log Ollama for user)  
- "Should I continue?" — **yes**, pick next queue item unless paused  

## Each iteration (one queue item per turn minimum)

1. **Check pause** — no `PAUSE_AUTONOMOUS`, user didn't stop  
2. **Read** `.cursor/plans/aether_autonomous_queue.plan.md` — top `pending` or `in_progress`  
3. If queue empty → scan near-term plan, grand design, recent failures → add 2–4 todos to queue  
4. Set todo **`in_progress`** in queue plan frontmatter  
5. **Implement** smallest complete slice (ship something testable)  
6. **Verify** (repo root):
   ```powershell
   cargo run -p aether_cli -- plane ensure --spawn
   cargo test -p aether_plane -p aether_package -p aether_cli --quiet
   cargo run -p aether_cli -- plane playtest minimal_explorer
   ```
   After generation changes: `plane smoke minimal_explorer --mode simulated`  
   After Dream prompt work: one `plane generate ... --mode dream --json` if Ollama up  
7. Mark todo **`completed`** (or `cancelled` with reason in session log)  
8. **Append** `.cursor/plans/aether_autonomous_session.md` (one short bullet: what shipped)  
9. Sync **near-term** plan if you completed something listed there  
10. **Continue** to next queue item in the same turn if budget allows; otherwise end turn with: what’s done + what’s next (no engagement bait)  

## Planning new work

- New scope → add todo to **autonomous queue**, not only chat  
- Larger than ~1 session → split into multiple queue todos  
- Defer MMO/3D/nested rooms unless user asked  

## Commits

- **Do not commit** unless user asked  
- Do commit if user said autonomous **and** "commit as you go"  

## User visibility

When ending a burst of work, report:

- Completed queue ids  
- Playtest/smoke result  
- Next up  
- How to pause: `New-Item .cursor/PAUSE_AUTONOMOUS` or say "pause"  

## Key paths

| File | Role |
|------|------|
| `aether_autonomous_queue.plan.md` | Backlog |
| `aether_autonomous_session.md` | Changelog |
| `aether_engine_near_term.plan.md` | Official milestones |
| `docs/agent-autoplay.md` | CLI reference |
