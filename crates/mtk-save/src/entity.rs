#![forbid(unsafe_code)]
//! Universal entity decoding for modern and legacy Minecraft saves.
//!
//! Parses entity compounds from chunk `Entities` lists in modern `entities/*.mca`
//! or legacy chunk `Entities` tags, converting supported visual entities (such as Paintings)
//! into high-level spatial representations.

use glam::{DVec3, IVec3};
use mtk_core::Direction;
use mtk_model::EntityInstance;

use std::io::Cursor;

use crate::error::SaveError;
use crate::nbt::{Nbt, NbtCompound};

pub(crate) struct EntityParser;

impl EntityParser {
    /// Parses an entities chunk NBT payload and extracts entities within the bounding box.
    pub(crate) fn parse_entities(
        nbt_data: &[u8],
        min_block: IVec3,
        max_block: IVec3,
    ) -> Result<Vec<EntityInstance>, SaveError> {
        let nbt = crate::nbt::read(&mut Cursor::new(nbt_data))?;
        let base = match nbt {
            Nbt::Some(b) => b,
            Nbt::None => return Ok(Vec::new()),
        };
        let root = base.as_compound();
        let entity_list = root
            .list("Entities")
            .or_else(|| root.list("entities"))
            .or_else(|| {
                root.compound("Level")
                    .and_then(|l| l.list("Entities").or_else(|| l.list("entities")))
            })
            .and_then(|l| l.compounds());

        let mut results = Vec::new();
        if let Some(entities) = entity_list {
            for comp in entities {
                if let Some(entity) = Self::parse_entity_compound(&comp, min_block, max_block) {
                    results.push(entity);
                }
            }
        }
        Ok(results)
    }

    /// Parses a single entity compound tag into an EntityInstance.
    pub(crate) fn parse_entity_compound(
        comp: &NbtCompound,
        min_block: IVec3,
        max_block: IVec3,
    ) -> Option<EntityInstance> {
        let id_str = comp
            .string("id")
            .or_else(|| comp.string("Id"))?
            .as_str()
            .to_string();
        let clean_id = id_str
            .strip_prefix("minecraft:")
            .unwrap_or(&id_str)
            .to_ascii_lowercase();

        match clean_id.as_str() {
            "painting" => Self::parse_painting(comp, min_block, max_block),
            "item_frame" | "glow_item_frame" => Self::parse_item_frame(comp, min_block, max_block),
            _ => None,
        }
    }

    fn parse_painting(
        comp: &NbtCompound,
        min_block: IVec3,
        max_block: IVec3,
    ) -> Option<EntityInstance> {
        // 1. Facing: byte/int (0: South, 1: West, 2: North, 3: East), string, or legacy Dir/Direction
        let facing = if let Some(b) = comp
            .byte("facing")
            .or_else(|| comp.byte("Facing"))
            .or_else(|| comp.byte("Dir"))
            .or_else(|| comp.byte("dir"))
            .or_else(|| comp.byte("Direction"))
            .or_else(|| comp.byte("direction"))
        {
            match b {
                0 => Direction::South,
                1 => Direction::West,
                2 => Direction::North,
                3 => Direction::East,
                _ => Direction::South,
            }
        } else if let Some(i) = comp
            .int("facing")
            .or_else(|| comp.int("Facing"))
            .or_else(|| comp.int("Dir"))
            .or_else(|| comp.int("dir"))
            .or_else(|| comp.int("Direction"))
            .or_else(|| comp.int("direction"))
        {
            match i {
                0 => Direction::South,
                1 => Direction::West,
                2 => Direction::North,
                3 => Direction::East,
                _ => Direction::South,
            }
        } else if let Some(s) = comp
            .string("facing")
            .or_else(|| comp.string("Facing"))
            .or_else(|| comp.string("Direction"))
            .or_else(|| comp.string("direction"))
        {
            Direction::parse_loose(s.as_str()).unwrap_or(Direction::South)
        } else {
            Direction::South
        };

        // 2. Variant / Motive
        let variant_raw = comp
            .string("variant")
            .or_else(|| comp.string("Variant"))
            .or_else(|| comp.string("Motive"))
            .or_else(|| comp.string("motive"))
            .map(|s| s.as_str().to_string())
            .unwrap_or_else(|| "kebab".to_string());
        let (variant_slug, width, height) =
            if let Some(v) = mtk_model::painting::get_painting_variant(&variant_raw) {
                (v.name.to_string(), v.width, v.height)
            } else {
                let slug =
                    mtk_model::painting::normalize_painting_name(&variant_raw).to_ascii_lowercase();
                let (w, h) = mtk_model::painting::painting_dimension(&slug);
                (slug, w, h)
            };

        // 3. Anchor block coordinates: block_pos (1.20.5+), TileX/Y/Z (legacy), or Pos list
        let block_pos = if let Some(arr) = comp
            .int_array("block_pos")
            .or_else(|| comp.int_array("BlockPos"))
        {
            if arr.len() == 3 {
                Some(IVec3::new(arr[0], arr[1], arr[2]))
            } else {
                None
            }
        } else if let (Some(x), Some(y), Some(z)) = (
            comp.int("TileX").or_else(|| comp.int("tile_x")),
            comp.int("TileY").or_else(|| comp.int("tile_y")),
            comp.int("TileZ").or_else(|| comp.int("tile_z")),
        ) {
            Some(IVec3::new(x, y, z))
        } else if let Some(pos_list) = comp.list("Pos").or_else(|| comp.list("pos")) {
            let coords = pos_list
                .doubles()
                .filter(|d| d.len() == 3)
                .map(|d| (d[0] as f32, d[1] as f32, d[2] as f32))
                .or_else(|| {
                    pos_list
                        .floats()
                        .filter(|f| f.len() == 3)
                        .map(|f| (f[0], f[1], f[2]))
                });

            coords.map(|(px, py, pz)| {
                // Pos = anchor + 0.5 - facing * 0.46875 + left * h_off + Y * v_off
                let h_off = if width % 2 == 0 { 0.5 } else { 0.0 };
                let v_off = if height % 2 == 0 { 0.5 } else { 0.0 };
                let left = match facing {
                    Direction::North => Direction::West,
                    Direction::East => Direction::North,
                    Direction::South => Direction::East,
                    Direction::West => Direction::South,
                    _ => Direction::North,
                };
                let left_norm = left.normal();
                let facing_norm = facing.normal();
                let anchor_x =
                    (px - 0.5 + facing_norm.x * 0.46875 - left_norm.x * h_off).round() as i32;
                let anchor_y = (py - 0.5 + facing_norm.y * 0.46875 - v_off).round() as i32;
                let anchor_z =
                    (pz - 0.5 + facing_norm.z * 0.46875 - left_norm.z * h_off).round() as i32;
                IVec3::new(anchor_x, anchor_y, anchor_z)
            })
        } else {
            None
        }?;

        // 4. 3D Bounding box intersection check
        // Compute the painting's full physical AABB to correctly include multi-block paintings
        let center = mtk_model::painting::calculate_painting_world_position(
            [block_pos.x, block_pos.y, block_pos.z],
            facing,
            width,
            height,
        );
        let h_extent = (width as f32) * 0.5;
        let v_extent = (height as f32) * 0.5;
        let (half_x, half_z) = match facing {
            Direction::East | Direction::West => (0.03125f32, h_extent),
            Direction::North | Direction::South => (h_extent, 0.03125f32),
            _ => (h_extent, h_extent),
        };
        let p_min_x = center.x - half_x;
        let p_max_x = center.x + half_x;
        let p_min_y = center.y - v_extent;
        let p_max_y = center.y + v_extent;
        let p_min_z = center.z - half_z;
        let p_max_z = center.z + half_z;

        let b_min_x = min_block.x as f32;
        let b_max_x = (max_block.x + 1) as f32;
        let b_min_y = min_block.y as f32;
        let b_max_y = (max_block.y + 1) as f32;
        let b_min_z = min_block.z as f32;
        let b_max_z = (max_block.z + 1) as f32;

        let aabb_overlaps = p_min_x < b_max_x
            && p_max_x > b_min_x
            && p_min_y < b_max_y
            && p_max_y > b_min_y
            && p_min_z < b_max_z
            && p_max_z > b_min_z;

        let pos_inside = block_pos.x >= min_block.x
            && block_pos.x <= max_block.x
            && block_pos.y >= min_block.y
            && block_pos.y <= max_block.y
            && block_pos.z >= min_block.z
            && block_pos.z <= max_block.z;

        if !aabb_overlaps && !pos_inside {
            return None;
        }

        Some(EntityInstance::new_painting(
            DVec3::new(block_pos.x as f64, block_pos.y as f64, block_pos.z as f64),
            facing,
            variant_slug,
            width,
            height,
        ))
    }

    fn parse_item_frame(
        comp: &NbtCompound,
        min_block: IVec3,
        max_block: IVec3,
    ) -> Option<EntityInstance> {
        // 1. Facing
        let facing = if let Some(b) = comp
            .byte("facing")
            .or_else(|| comp.byte("Facing"))
            .or_else(|| comp.byte("Direction"))
        {
            match b {
                0 => Direction::Down,
                1 => Direction::Up,
                2 => Direction::North,
                3 => Direction::South,
                4 => Direction::West,
                5 => Direction::East,
                _ => Direction::North,
            }
        } else {
            Direction::North
        };

        // 2. Position
        let pos = if let Some(tile_x) = comp.int("TileX") {
            let tile_y = comp.int("TileY").unwrap_or(0);
            let tile_z = comp.int("TileZ").unwrap_or(0);
            IVec3::new(tile_x, tile_y, tile_z)
        } else {
            let pos_list = comp.list("Pos").and_then(|l| l.doubles())?;
            if pos_list.len() >= 3 {
                IVec3::new(
                    pos_list[0].floor() as i32,
                    pos_list[1].floor() as i32,
                    pos_list[2].floor() as i32,
                )
            } else {
                return None;
            }
        };

        if pos.x < min_block.x
            || pos.x > max_block.x
            || pos.y < min_block.y
            || pos.y > max_block.y
            || pos.z < min_block.z
            || pos.z > max_block.z
        {
            return None;
        }

        // 3. Item payload
        let item_comp = comp.compound("Item").or_else(|| comp.compound("item"));
        let item_id = item_comp
            .and_then(|ic| ic.string("id").or_else(|| ic.string("Id")))
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        let rotation = comp
            .byte("ItemRotation")
            .or_else(|| comp.byte("item_rotation"))
            .unwrap_or(0) as u8;

        let is_glowing = comp
            .string("id")
            .map(|s| s.contains("glow"))
            .unwrap_or(false);

        let is_invisible = comp
            .byte("Invisible")
            .or_else(|| comp.byte("invisible"))
            .map(|b| b != 0)
            .unwrap_or(false);

        Some(EntityInstance::new_item_frame(
            DVec3::new(pos.x as f64, pos.y as f64, pos.z as f64),
            facing,
            item_id,
            rotation,
            is_glowing,
            is_invisible,
            None,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::{NbtCompound, NbtList, NbtStr, NbtTag};

    #[test]
    fn test_parse_modern_painting_with_block_pos() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:painting")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("variant"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:unpacked")),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("facing"), NbtTag::Byte(3))); // East
        let bp_bytes = [
            (-8i32).to_be_bytes(),
            (-59i32).to_be_bytes(),
            10i32.to_be_bytes(),
        ]
        .concat();
        comp.entries.push((
            NbtStr::from_borrowed("block_pos"),
            NbtTag::IntArray(&bp_bytes),
        ));

        let entity = EntityParser::parse_entity_compound(
            &comp,
            IVec3::new(-17, -61, -17),
            IVec3::new(19, -48, 17),
        )
        .expect("Should parse modern painting");

        assert_eq!(entity.facing, Some(Direction::East));
        assert_eq!(entity.position, DVec3::new(-8.0, -59.0, 10.0));
        match entity.kind {
            mtk_model::EntityKindData::Painting(p) => {
                assert_eq!(p.variant, "unpacked");
                assert_eq!(p.width, 4);
                assert_eq!(p.height, 4);
            }
            _ => panic!("Expected Painting"),
        }
    }

    #[test]
    fn test_parse_legacy_painting_with_tile_coords() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("Painting")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("Motive"),
            NbtTag::String(NbtStr::from_borrowed("Kebab")),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("Facing"), NbtTag::Byte(2))); // North
        comp.entries
            .push((NbtStr::from_borrowed("TileX"), NbtTag::Int(10)));
        comp.entries
            .push((NbtStr::from_borrowed("TileY"), NbtTag::Int(64)));
        comp.entries
            .push((NbtStr::from_borrowed("TileZ"), NbtTag::Int(20)));

        let entity = EntityParser::parse_entity_compound(
            &comp,
            IVec3::new(0, 0, 0),
            IVec3::new(50, 100, 50),
        )
        .expect("Should parse legacy painting");

        assert_eq!(entity.facing, Some(Direction::North));
        assert_eq!(entity.position, DVec3::new(10.0, 64.0, 20.0));
        match entity.kind {
            mtk_model::EntityKindData::Painting(p) => {
                assert_eq!(p.variant, "kebab");
                assert_eq!(p.width, 1);
                assert_eq!(p.height, 1);
            }
            _ => panic!("Expected Painting"),
        }
    }

    #[test]
    fn test_parse_fallback_pos_reconstructs_anchor() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("painting")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("variant"),
            NbtTag::String(NbtStr::from_borrowed("pool")),
        )); // 2x1
        comp.entries
            .push((NbtStr::from_borrowed("facing"), NbtTag::Byte(3))); // East
        comp.entries.push((
            NbtStr::from_borrowed("Pos"),
            NbtTag::List(NbtList::Double(vec![-7.96875, -56.5, 7.0])),
        ));

        let entity = EntityParser::parse_entity_compound(
            &comp,
            IVec3::new(-17, -61, -17),
            IVec3::new(19, -48, 17),
        )
        .expect("Should reconstruct anchor from Pos");

        assert_eq!(entity.facing, Some(Direction::East));
        assert_eq!(entity.position, DVec3::new(-8.0, -57.0, 7.0));
        match entity.kind {
            mtk_model::EntityKindData::Painting(p) => {
                assert_eq!(p.variant, "pool");
                assert_eq!(p.width, 2);
                assert_eq!(p.height, 1);
            }
            _ => panic!("Expected Painting"),
        }
    }

    #[test]
    fn test_painting_outside_bounds_is_rejected() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("painting")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("variant"),
            NbtTag::String(NbtStr::from_borrowed("kebab")),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("facing"), NbtTag::Byte(0)));
        let bp_bytes = [
            100i32.to_be_bytes(),
            64i32.to_be_bytes(),
            100i32.to_be_bytes(),
        ]
        .concat();
        comp.entries.push((
            NbtStr::from_borrowed("block_pos"),
            NbtTag::IntArray(&bp_bytes),
        ));

        let entity =
            EntityParser::parse_entity_compound(&comp, IVec3::new(0, 0, 0), IVec3::new(16, 16, 16));
        assert!(entity.is_none(), "Outside bounds should be rejected");
    }
}
