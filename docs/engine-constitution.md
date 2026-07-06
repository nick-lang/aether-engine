# Engine Constitution

Design principles for Aether Engine games and tooling.

## 1. Exploration must always pay off

Curiosity yields immediate, mid-term, and long-term rewards. Failed attempts still produce information that changes future decisions.

**Engine:** `Mechanic` types support `on_event` → director signal → content opportunity loops.

## 2. Hidden progression emerges from playstyle

Unlocks follow behavior patterns over time, not menu picks. Rare unlocks require multi-domain mastery.

**Engine:** `BehaviorProfile` components and World Director rules drive generation triggers.

## 3. Persistence makes the world feel alive

Generated content flows: candidate → validate → canonical → versioned history with rollback.

**Engine:** Canonical package store and append-only event log; runtime consumes approved patches only.

## 4. AI is a creative accelerator, not the final arbiter

All AI output is structured, schema-valid, and rule-checked. High-impact content can require human review.

**Engine:** No publish path bypasses validation.

## 5. Reward economy stays stable

Budget caps, anti-farm logic, and novelty checks protect against exploit-driven inflation.

**Engine:** Shared `RewardBudget` rule module across games.

## 6. Layered ambition

1. Single-player package runtime (Phase 1)
2. Live content plane + patches (Phase 2)
3. AI authoring studio (Phase 3)
4. Editor and multiplayer (Phase 4)
