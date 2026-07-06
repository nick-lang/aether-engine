//! Near-term incremental checklist — simulated (keyword) path.
//!
//! Manual Ollama runs use the same prompts in Studio; these tests guard regression
//! without calling the model.

#[cfg(test)]
mod tests {
    use aether_package::load_package;
    use aether_patch::PatchDocument;

    use super::super::incremental_plan::parse_plan_from_keywords;
    use super::super::llm_validate::validate_llm_patch;
    use super::super::plan_builder::plan_to_patch;
    use super::super::simulated::generate_from_prompt;

    fn canonical() -> aether_package::GamePackage {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/minimal_explorer/package.json");
        load_package(path).expect("load")
    }

    fn sim(prompt: &str) -> PatchDocument {
        let c = canonical();
        generate_from_prompt(prompt, 1, "main", "minimal_explorer", &c, &[]).expect(prompt)
    }

    #[test]
    fn checklist_01_shard_east() {
        let patch = sim("add a shard to the east");
        assert_eq!(patch.upsert_entity_ops().count(), 1);
    }

    #[test]
    fn checklist_02_shard_north() {
        let patch = sim("add a collectible to the north");
        assert_eq!(patch.upsert_entity_ops().count(), 1);
    }

    #[test]
    fn checklist_03_room_north_aligns() {
        let patch = sim("add a room to the north");
        let room = patch.upsert_room_ops().next().expect("room");
        let y = room.room["bounds"]["y"].as_f64().expect("y");
        assert_eq!(y, 140.0);
        assert!(patch.paint_tiles_ops().any(|p| p.fill.as_deref() == Some("moss")));
    }

    #[test]
    fn checklist_04_room_west() {
        let patch = sim("add a room to the west");
        assert_eq!(patch.upsert_room_ops().count(), 1);
    }

    #[test]
    fn checklist_06_second_shard_unique_collectible() {
        let patch = sim("add another golden shard to the east");
        let op = patch.upsert_entity_ops().next().expect("entity");
        let cid = op.entity["components"]["collectible"]["id"]
            .as_str()
            .expect("collectible id");
        assert_ne!(cid, "grove_shard");
    }

    #[test]
    fn checklist_07_sprite_patch_rejected() {
        let patch = PatchDocument {
            patch_id: "bad".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "upsert_entity",
                "scene": "main",
                "entity": {
                    "id": "prop",
                    "scene": "main",
                    "components": {
                        "sprite": { "asset": "x" },
                        "transform": { "x": 0, "y": 0 }
                    }
                }
            })],
        };
        assert!(validate_llm_patch(&patch, "main", &canonical()).is_err());
    }

    #[test]
    fn checklist_08_moss_floor_no_extra_room() {
        let plan = parse_plan_from_keywords("mossy floor in the north room").expect("plan");
        let patch = plan_to_patch(&plan, 1, "main", &canonical(), &[]).expect("patch");
        assert_eq!(patch.upsert_room_ops().count(), 0);
        assert!(patch.paint_tiles_ops().any(|p| p.fill.as_deref() == Some("moss")));
    }

    #[test]
    fn checklist_09_paint_unknown_room_rejected() {
        let patch = PatchDocument {
            patch_id: "bad_paint".into(),
            base_version: 1,
            ops: vec![serde_json::json!({
                "op": "paint_tiles",
                "scene": "main",
                "room_id": "nonexistent_room",
                "tile_size": 16.0,
                "fill": "moss"
            })],
        };
        assert!(validate_llm_patch(&patch, "main", &canonical()).is_err());
    }
}
