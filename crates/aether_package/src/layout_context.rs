//! Merged room layout for generation (canonical + pending candidates + in-patch drafts).

use aether_core::EngineResult;
use aether_patch::PatchDocument;

use crate::{
    default_main_room_bounds, room_is_beyond, rooms_share_edge, Direction, GamePackage,
    RoomBoundsDef, RoomDef,
};

/// Room bounds used when placing new rooms/entities during generation.
#[derive(Debug, Clone, Default)]
pub struct LayoutContext {
    rooms: Vec<(String, RoomBoundsDef)>,
}

impl LayoutContext {
    pub fn from_canonical(package: &GamePackage, scene_id: &str) -> Self {
        let rooms = package
            .rooms_for_scene(scene_id)
            .into_iter()
            .map(|r| (r.id.clone(), r.bounds.clone()))
            .collect();
        Self { rooms }
    }

    pub fn apply_patches(
        &mut self,
        patches: &[PatchDocument],
        scene_id: &str,
    ) -> EngineResult<()> {
        for patch in patches {
            self.apply_patch(patch, scene_id)?;
        }
        Ok(())
    }

    pub fn apply_patch(&mut self, patch: &PatchDocument, scene_id: &str) -> EngineResult<()> {
        for op in patch.upsert_room_ops() {
            if op.scene != scene_id {
                continue;
            }
            let room: RoomDef = serde_json::from_value(op.room).map_err(|e| {
                aether_core::EngineError::InvalidPackage(format!("layout room op: {e}"))
            })?;
            self.upsert_room(room.id, room.bounds);
        }
        Ok(())
    }

    pub fn upsert_room(&mut self, id: impl Into<String>, bounds: RoomBoundsDef) {
        let id = id.into();
        if let Some(entry) = self.rooms.iter_mut().find(|(rid, _)| rid == &id) {
            entry.1 = bounds;
        } else {
            self.rooms.push((id, bounds));
        }
    }

    pub fn push_room(&mut self, id: impl Into<String>, bounds: RoomBoundsDef) {
        self.upsert_room(id, bounds);
    }

    pub fn bounds_for_scene(&self) -> Vec<RoomBoundsDef> {
        self.rooms.iter().map(|(_, b)| b.clone()).collect()
    }

    pub fn find_room_bounds(&self, room_id: &str) -> Option<&RoomBoundsDef> {
        self.rooms
            .iter()
            .find(|(id, _)| id == room_id)
            .map(|(_, b)| b)
    }

    /// Room farthest along `direction` from the scene anchor (for tile paint / props).
    pub fn extremal_room_in_direction(
        &self,
        direction: Direction,
    ) -> Option<(String, RoomBoundsDef)> {
        let anchor = self.reference_bounds();
        let mut best: Option<(String, RoomBoundsDef)> = None;
        for (id, bounds) in &self.rooms {
            let is_candidate = room_is_beyond(bounds, &anchor, direction)
                || rooms_share_edge(&anchor, bounds, direction);
            if !is_candidate {
                continue;
            }
            let replace = match &best {
                None => true,
                Some((_, b)) => is_farther(bounds, b, direction),
            };
            if replace {
                best = Some((id.clone(), bounds.clone()));
            }
        }
        best
    }

    fn reference_bounds(&self) -> RoomBoundsDef {
        self.bounds_for_scene()
            .into_iter()
            .max_by(|a, b| {
                a.area()
                    .partial_cmp(&b.area())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .unwrap_or_else(default_main_room_bounds)
    }
}

fn is_farther(candidate: &RoomBoundsDef, current: &RoomBoundsDef, direction: Direction) -> bool {
    match direction {
        Direction::North => candidate.max_y() > current.max_y(),
        Direction::South => candidate.min_y() < current.min_y(),
        Direction::East => candidate.max_x() > current.max_x(),
        Direction::West => candidate.min_x() < current.min_x(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_patch::load_patch;

    #[test]
    fn pending_patch_extends_layout() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        let package = crate::load_package(path).expect("load");
        let pending = vec![load_patch(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../examples/minimal_explorer/patches/add_north_room.json"),
        )
        .expect("patch")];
        let mut ctx = LayoutContext::from_canonical(&package, "main");
        ctx.apply_patches(&pending, "main").expect("apply");
        let north = ctx
            .find_room_bounds("north_grove")
            .expect("north in layout");
        assert!((north.min_y() - 140.0).abs() < 1.0);
    }
}
