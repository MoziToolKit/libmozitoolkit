//! Identical input must serialize to identical bytes.
//!
//! The address map is embedded in the asset cache and in scene packages. `HashMap` iteration
//! order is randomized per map instance, so serializing it directly makes two bakes of the same
//! pack produce different packages — which breaks reproducible builds, byte-level cache diffing
//! and any "is this cache the one I just baked?" comparison.

use mtk_resource::ResourceLocation;
use mtk_texture::{
    AtlasAddressMap, AtlasBuilder, AtlasBuilderConfig, AtlasSpriteLocation, DecodedSprite,
    RgbaBuffer,
};

fn sprite(index: usize) -> DecodedSprite {
    let id = ResourceLocation::new("minecraft", format!("block/determinism_{index:03}"));
    DecodedSprite {
        sprite_id: id,
        albedo: RgbaBuffer::solid(16, 16, (index % 251) as u8, 128, 64, 255),
        normal: None,
        specular: None,
        overlay: None,
        emissive: None,
        normal_gl: None,
        orm: None,
        aux: None,
        frame_width: 16,
        frame_height: 16,
        frame_count: 1,
        metadata: None,
    }
}

fn bake_serialized(sprites: Vec<DecodedSprite>) -> Vec<u8> {
    let baked = AtlasBuilder::new(AtlasBuilderConfig::production(1024, 1024, 0))
        .build_from_sprites_with_category(sprites, "blocks")
        .expect("bake");
    serde_json::to_vec(&baked.address_map).expect("serialize address map")
}

#[test]
fn baking_the_same_sprites_twice_serializes_to_identical_bytes() {
    let sprites: Vec<DecodedSprite> = (0..64).map(sprite).collect();
    let first = bake_serialized(sprites.clone());
    let second = bake_serialized(sprites);

    assert_eq!(
        first.len(),
        second.len(),
        "same input produced address maps of different length"
    );
    assert!(
        first == second,
        "two bakes of the same sprites produced different bytes: the address map serializes in \
         hash order, so caches are not reproducible"
    );
}

#[test]
fn a_hand_built_map_serializes_in_key_order() {
    let mut forward = AtlasAddressMap::new();
    let mut backward = AtlasAddressMap::new();
    let ids: Vec<ResourceLocation> = (0..32)
        .map(|i| ResourceLocation::new("minecraft", format!("block/order_{i:02}")))
        .collect();

    for (i, id) in ids.iter().enumerate() {
        let loc = AtlasSpriteLocation {
            texture_id: i as u32,
            ..Default::default()
        };
        forward.sprites.insert(id.clone(), loc.clone());
    }
    for (i, id) in ids.iter().enumerate().rev() {
        let loc = AtlasSpriteLocation {
            texture_id: i as u32,
            ..Default::default()
        };
        backward.sprites.insert(id.clone(), loc);
    }

    let a = serde_json::to_string(&forward).expect("serialize");
    let b = serde_json::to_string(&backward).expect("serialize");
    assert_eq!(a, b, "insertion order must not affect the serialized bytes");

    // Keys must appear in a stable (lexicographic) order, not the insertion order.
    let first_key = forward.sprites.keys().min().expect("non-empty").as_string();
    let serialized = a;
    let key_positions: Vec<usize> = {
        let mut positions = Vec::new();
        for id in &ids {
            let needle = format!("\"{}\"", id.as_string());
            positions.push(serialized.find(&needle).expect("key present"));
        }
        positions
    };
    let mut sorted_positions = key_positions.clone();
    sorted_positions.sort_unstable();
    assert_eq!(
        key_positions, sorted_positions,
        "serialized keys must be ordered; got hash order starting at {first_key}"
    );
}
