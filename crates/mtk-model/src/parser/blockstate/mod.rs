//! # BlockState Parser and Variant Resolver
//!
//! Provides parsing of Minecraft canonical BlockState identifiers and
//! matching logic against `blockstates/*.json` variant definitions and multipart rules.

mod collapse;
pub(crate) mod definition;
pub(crate) mod resolver;
pub(crate) mod state;

pub use definition::*;
pub use resolver::*;
pub use state::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple() {
        let bs = BlockState::parse("stone").unwrap();
        assert_eq!(bs.namespace, "minecraft");
        assert_eq!(bs.name, "stone");
        assert!(bs.properties.is_empty());
        assert_eq!(bs.to_canonical_string(), "minecraft:stone");
    }

    #[test]
    fn test_parse_with_properties() {
        let input = "minecraft:observer[powered=false,facing=north]";
        let bs = BlockState::parse(input).unwrap();
        assert_eq!(bs.block_id(), "minecraft:observer");
        assert_eq!(bs.properties.get("facing").unwrap(), "north");
        assert_eq!(bs.properties.get("powered").unwrap(), "false");
        assert_eq!(
            bs.to_canonical_string(),
            "minecraft:observer[facing=north,powered=false]"
        );
    }

    #[test]
    fn test_matches_properties() {
        let bs = BlockState::parse("minecraft:oak_stairs[facing=east,half=bottom,shape=straight]")
            .unwrap();
        assert!(bs.matches_properties([("facing", "east"), ("half", "bottom")]));
        assert!(!bs.matches_properties([("facing", "west")]));
    }

    #[test]
    fn test_resolve_variants() {
        let json_data = r#"{
            "variants": {
                "facing=north": { "model": "minecraft:block/furnace" },
                "facing=south": { "model": "minecraft:block/furnace", "y": 180 },
                "facing=west":  { "model": "minecraft:block/furnace", "y": 270 },
                "facing=east":  { "model": "minecraft:block/furnace", "y": 90 }
            }
        }"#;
        let def: BlockStateDefinition = serde_json::from_str(json_data).unwrap();
        let bs = BlockState::parse("minecraft:furnace[facing=east]").unwrap();
        let matches = BlockStateResolver::resolve(&def, &bs);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].model_id, "minecraft:block/furnace");
        assert_eq!(matches[0].rot_y, 90.0);
    }

    #[test]
    fn test_resolve_multipart_conditions() {
        let json_data = r#"{
            "multipart": [
                {
                    "apply": { "model": "minecraft:block/oak_fence_post" }
                },
                {
                    "when": { "north": "true" },
                    "apply": { "model": "minecraft:block/oak_fence_side", "uvlock": true }
                },
                {
                    "when": { "OR": [{ "south": "true" }, { "east": "true" }] },
                    "apply": { "model": "minecraft:block/oak_fence_side", "y": 90 }
                }
            ]
        }"#;
        let def: BlockStateDefinition = serde_json::from_str(json_data).unwrap();
        let bs =
            BlockState::parse("minecraft:oak_fence[north=true,south=false,east=true]").unwrap();
        let matches = BlockStateResolver::resolve(&def, &bs);
        // Should match post (unconditional), north=true, and OR (east=true) -> 3 models
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0].model_id, "minecraft:block/oak_fence_post");
        assert_eq!(matches[1].model_id, "minecraft:block/oak_fence_side");
        assert!(matches[1].uvlock);
        assert_eq!(matches[2].rot_y, 90.0);
    }

    #[test]
    fn test_resolve_chiseled_bookshelf_and_condition() {
        let json_data = r#"{
            "multipart": [
                {
                    "apply": { "model": "minecraft:block/chiseled_bookshelf", "y": 180 },
                    "when": { "facing": "south" }
                },
                {
                    "apply": { "model": "minecraft:block/chiseled_bookshelf_occupied_slot_top_left", "y": 180 },
                    "when": {
                        "AND": [
                            { "facing": "south" },
                            { "slot_0_occupied": "true" }
                        ]
                    }
                },
                {
                    "apply": { "model": "minecraft:block/chiseled_bookshelf_empty_slot_top_left", "y": 180 },
                    "when": {
                        "AND": [
                            { "facing": "south" },
                            { "slot_0_occupied": "false" }
                        ]
                    }
                }
            ]
        }"#;
        let def: BlockStateDefinition = serde_json::from_str(json_data).unwrap();
        let bs_occupied =
            BlockState::parse("minecraft:chiseled_bookshelf[facing=south,slot_0_occupied=true]")
                .unwrap();
        let matches_occ = BlockStateResolver::resolve(&def, &bs_occupied);
        assert_eq!(matches_occ.len(), 2);
        assert_eq!(
            matches_occ[0].model_id,
            "minecraft:block/chiseled_bookshelf"
        );
        assert_eq!(
            matches_occ[1].model_id,
            "minecraft:block/chiseled_bookshelf_occupied_slot_top_left"
        );

        let bs_empty =
            BlockState::parse("minecraft:chiseled_bookshelf[facing=south,slot_0_occupied=false]")
                .unwrap();
        let matches_emp = BlockStateResolver::resolve(&def, &bs_empty);
        assert_eq!(matches_emp.len(), 2);
        assert_eq!(
            matches_emp[0].model_id,
            "minecraft:block/chiseled_bookshelf"
        );
        assert_eq!(
            matches_emp[1].model_id,
            "minecraft:block/chiseled_bookshelf_empty_slot_top_left"
        );
    }

    #[test]
    fn test_resolve_wildflowers_flower_amount_and_flowers_alias() {
        let json_data = r#"{
            "multipart": [
                {
                    "apply": { "model": "minecraft:block/wildflowers_1" },
                    "when": { "facing": "north" }
                },
                {
                    "apply": { "model": "minecraft:block/wildflowers_2" },
                    "when": { "facing": "north", "flower_amount": "2|3|4" }
                }
            ]
        }"#;
        let def: BlockStateDefinition = serde_json::from_str(json_data).unwrap();

        // 1 flower -> only wildflowers_1
        let bs1 = BlockState::parse("minecraft:wildflowers[facing=north,flower_amount=1]").unwrap();
        let m1 = BlockStateResolver::resolve(&def, &bs1);
        assert_eq!(m1.len(), 1);
        assert_eq!(m1[0].model_id, "minecraft:block/wildflowers_1");

        // 2 flowers -> wildflowers_1 and wildflowers_2
        let bs2 = BlockState::parse("minecraft:wildflowers[facing=north,flower_amount=2]").unwrap();
        let m2 = BlockStateResolver::resolve(&def, &bs2);
        assert_eq!(m2.len(), 2);
        assert_eq!(m2[0].model_id, "minecraft:block/wildflowers_1");
        assert_eq!(m2[1].model_id, "minecraft:block/wildflowers_2");

        // Backward compatibility alias: "flowers=2" matches "flower_amount: 2|3|4"
        let bs_alias = BlockState::parse("minecraft:wildflowers[facing=north,flowers=2]").unwrap();
        let m_alias = BlockStateResolver::resolve(&def, &bs_alias);
        assert_eq!(m_alias.len(), 2);
        assert_eq!(m_alias[1].model_id, "minecraft:block/wildflowers_2");
    }

    #[test]
    fn test_collapse_invariant_variant_props_for_datagen_tracks_and_preserves_meaningful_ones() {
        // 1. Track-like datagen variants where waterlogged & turn never change the model
        let track_json = r#"{
            "variants": {
                "shape=ae,turn=false,waterlogged=false": { "model": "mod:block/track/ascending", "y": 270 },
                "shape=ae,turn=false,waterlogged=true":  { "model": "mod:block/track/ascending", "y": 270 },
                "shape=ae,turn=true,waterlogged=false":  { "model": "mod:block/track/ascending", "y": 270 },
                "shape=ae,turn=true,waterlogged=true":   { "model": "mod:block/track/ascending", "y": 270 },
                "shape=cr_d,turn=false,waterlogged=false": { "model": "mod:block/track/cross_d" },
                "shape=cr_d,turn=false,waterlogged=true":  { "model": "mod:block/track/cross_d" },
                "shape=cr_d,turn=true,waterlogged=false":  { "model": "mod:block/track/cross_d" },
                "shape=cr_d,turn=true,waterlogged=true":   { "model": "mod:block/track/cross_d" }
            }
        }"#;
        let track_def: BlockStateDefinition = serde_json::from_str(track_json).unwrap();
        let collapsed = track_def.enumerate_all_states("mod:track");
        assert_eq!(
            collapsed,
            vec!["mod:track[shape=ae]", "mod:track[shape=cr_d]"]
        );

        // Resolver still maps folded states to the exact VariantMatch
        let folded_ae = BlockState::parse("mod:track[shape=ae]").unwrap();
        let resolved = BlockStateResolver::resolve_variants(&track_def, &folded_ae);
        assert_eq!(resolved[0].0[0].model_id, "mod:block/track/ascending");
        assert_eq!(resolved[0].0[0].rot_y, 270.0);

        // 2. Vanilla sea_pickle-like definition where waterlogged DOES change the model must NOT collapse
        let pickle_json = r#"{
            "variants": {
                "pickles=1,waterlogged=false": { "model": "minecraft:block/sea_pickle_dead" },
                "pickles=1,waterlogged=true":  { "model": "minecraft:block/sea_pickle" }
            }
        }"#;
        let pickle_def: BlockStateDefinition = serde_json::from_str(pickle_json).unwrap();
        assert_eq!(
            pickle_def.enumerate_all_states("minecraft:sea_pickle"),
            vec![
                "minecraft:sea_pickle[pickles=1,waterlogged=false]",
                "minecraft:sea_pickle[pickles=1,waterlogged=true]"
            ]
        );

        // 3. Emissive property "lit" must be preserved even when model references match
        let lit_json = r#"{
            "variants": {
                "lit=false": { "model": "mod:block/lamp" },
                "lit=true":  { "model": "mod:block/lamp" }
            }
        }"#;
        let lit_def: BlockStateDefinition = serde_json::from_str(lit_json).unwrap();
        assert_eq!(
            lit_def.enumerate_all_states("mod:lamp"),
            vec!["mod:lamp[lit=false]", "mod:lamp[lit=true]"]
        );
    }
}
