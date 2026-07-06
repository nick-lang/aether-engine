//! Rule engine — guardrails before patches become candidates.

use aether_core::{EngineError, EngineResult};
use aether_patch::PatchDocument;

const MAX_OPS: usize = 32;
const MAX_OPS_DREAM: usize = 48;

pub fn validate_patch_rules(patch: &PatchDocument) -> EngineResult<()> {
    validate_patch_rules_with_limit(patch, MAX_OPS)
}

pub fn validate_patch_rules_dream(patch: &PatchDocument) -> EngineResult<()> {
    validate_patch_rules_with_limit(patch, MAX_OPS_DREAM)
}

fn validate_patch_rules_with_limit(patch: &PatchDocument, max_ops: usize) -> EngineResult<()> {
    if patch.patch_id.trim().is_empty() {
        return Err(EngineError::PatchRejected("patch_id must not be empty".into()));
    }
    if patch.ops.is_empty() {
        return Err(EngineError::PatchRejected("patch must have at least one op".into()));
    }
    if patch.ops.len() > max_ops {
        return Err(EngineError::PatchRejected(format!(
            "patch exceeds max {max_ops} ops"
        )));
    }
    for op in &patch.ops {
        if !aether_patch::is_supported_op(op) {
            return Err(EngineError::PatchRejected(
                "unsupported op: only upsert_entity, upsert_room, and paint_tiles are allowed".into(),
            ));
        }
    }
    Ok(())
}
