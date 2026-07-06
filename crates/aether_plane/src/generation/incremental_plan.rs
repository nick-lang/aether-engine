//! Small JSON plan schema — LLM fills this; Rust builds [`PatchDocument`].

use aether_core::{EngineError, EngineResult};
use serde::{Deserialize, Serialize};

use super::repair::parse_json_value;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IncrementalPlan {
    pub actions: Vec<PlanAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PlanAction {
    AddRoom {
        direction: String,
        #[serde(default)]
        label: Option<String>,
    },
    AddCollectible {
        direction: String,
        #[serde(default)]
        id: Option<String>,
        #[serde(default)]
        label: Option<String>,
    },
    PaintTiles {
        direction: String,
        tile: String,
        #[serde(default)]
        room_id: Option<String>,
    },
}

pub fn parse_plan_json(raw: &str) -> EngineResult<IncrementalPlan> {
    let value = parse_json_value(raw)?;
    serde_json::from_value(value)
        .map_err(|e| EngineError::InvalidPackage(format!("plan schema invalid: {e}")))
}

/// Deterministic fallback when the model returns bad JSON.
pub fn parse_plan_from_keywords(prompt: &str) -> Option<IncrementalPlan> {
    let lower = prompt.to_lowercase();
    let mut actions = Vec::new();

    let direction = if lower.contains("north") {
        "north"
    } else if lower.contains("south") {
        "south"
    } else if lower.contains("east") {
        "east"
    } else if lower.contains("west") {
        "west"
    } else {
        "center"
    };

    let wants_paint = lower.contains("tile")
        || lower.contains("floor")
        || lower.contains("moss")
        || lower.contains("grass")
        || lower.contains("wall");
    let wants_new_room = (lower.contains("add") || lower.contains("new") || lower.contains("create"))
        && lower.contains("room")
        && !lower.contains("paint");
    if wants_new_room || (lower.contains("room") && !wants_paint) {
        actions.push(PlanAction::AddRoom {
            direction: direction.into(),
            label: None,
        });
    }

    if lower.contains("shard")
        || lower.contains("collectible")
        || lower.contains("treasure")
        || lower.contains("pickup")
        || (lower.contains("grove") && !lower.contains("room"))
    {
        let id = if lower.contains("golden") {
            Some("golden_shard".into())
        } else if lower.contains("grove") {
            Some("grove_shard".into())
        } else {
            None
        };
        actions.push(PlanAction::AddCollectible {
            direction: direction.into(),
            id,
            label: None,
        });
    }

    if wants_paint {
        let tile = if lower.contains("moss") {
            "moss"
        } else if lower.contains("grass") {
            "grass"
        } else if lower.contains("wall") {
            "wall"
        } else {
            "floor"
        };
        actions.push(PlanAction::PaintTiles {
            direction: direction.into(),
            tile: tile.into(),
            room_id: None,
        });
    }

    if actions.is_empty() {
        None
    } else {
        Some(IncrementalPlan { actions })
    }
}

pub fn validate_plan(plan: &IncrementalPlan) -> EngineResult<()> {
    if plan.actions.is_empty() {
        return Err(EngineError::PatchRejected(
            "plan must include at least one action".into(),
        ));
    }
    if plan.actions.len() > 8 {
        return Err(EngineError::PatchRejected(
            "plan exceeds max 8 actions".into(),
        ));
    }
    for action in &plan.actions {
        match action {
            PlanAction::AddRoom { direction, .. }
            | PlanAction::AddCollectible { direction, .. }
            | PlanAction::PaintTiles { direction, .. } => {
                super::spatial::normalize_direction(direction);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plan_json() {
        let raw = r#"{"actions":[{"type":"add_room","direction":"north"}]}"#;
        let plan = parse_plan_json(raw).expect("parse");
        assert_eq!(plan.actions.len(), 1);
    }

    #[test]
    fn parses_paint_tiles_action() {
        let raw = r#"{"actions":[{"type":"paint_tiles","direction":"north","tile":"moss"}]}"#;
        let plan = parse_plan_json(raw).expect("parse");
        assert!(matches!(
            plan.actions[0],
            PlanAction::PaintTiles { ref tile, .. } if tile == "moss"
        ));
    }

    #[test]
    fn moss_floor_does_not_add_room() {
        let plan = parse_plan_from_keywords("mossy floor in the north room").expect("plan");
        assert!(plan.actions.iter().all(|a| !matches!(a, PlanAction::AddRoom { .. })));
        assert!(plan.actions.iter().any(|a| matches!(a, PlanAction::PaintTiles { .. })));
    }

    #[test]
    fn keyword_fallback_shard() {
        let plan = parse_plan_from_keywords("add a golden shard to the east").expect("plan");
        assert!(matches!(plan.actions[0], PlanAction::AddCollectible { .. }));
    }
}
