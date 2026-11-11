#![forbid(unsafe_code)]
//! Universal entity decoding for modern and legacy Minecraft saves.
//!
//! Parses entity compounds from chunk `Entities` lists in modern `entities/*.mca`
//! or legacy chunk `Entities` tags, converting visual entities (such as Paintings and Item Frames)
//! into high-level spatial representations with full float precision and UUID retention.

use glam::{DVec3, IVec3, Vec3};
use mtk_core::Direction;
use mtk_model::EntityInstance;
#[cfg(test)]
use mtk_model::EntityKindData;

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
        let id_lower = id_str.to_ascii_lowercase();
        let clean_id = id_lower.strip_prefix("minecraft:").unwrap_or(&id_lower);

        match clean_id {
            "painting" => Self::parse_painting(comp, min_block, max_block),
            "item_frame" | "glow_item_frame" => Self::parse_item_frame(comp, min_block, max_block),
            _ => None,
        }
    }

    /// Extracts UUID from modern IntArray or legacy Least/Most tags.
    pub(crate) fn parse_uuid(comp: &NbtCompound) -> Option<String> {
        if let Some(arr) = comp.int_array("UUID").or_else(|| comp.int_array("uuid")) {
            if arr.len() == 4 {
                let u0 = arr[0] as u32;
                let u1 = arr[1] as u32;
                let u2 = arr[2] as u32;
                let u3 = arr[3] as u32;
                return Some(format!(
                    "{:08x}-{:04x}-{:04x}-{:04x}-{:04x}{:08x}",
                    u0,
                    (u1 >> 16) as u16,
                    u1 as u16,
                    (u2 >> 16) as u16,
                    u2 as u16,
                    u3
                ));
            }
        }
        if let (Some(most), Some(least)) = (
            comp.long("UUIDMost").or_else(|| comp.long("uuid_most")),
            comp.long("UUIDLeast").or_else(|| comp.long("uuid_least")),
        ) {
            let u_most = most as u64;
            let u_least = least as u64;
            return Some(format!(
                "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
                (u_most >> 32) as u32,
                (u_most >> 16) as u16,
                u_most as u16,
                (u_least >> 48) as u16,
                u_least & 0x0000_ffff_ffff_ffff
            ));
        }
        if let Some(s) = comp.string("UUID").or_else(|| comp.string("uuid")) {
            return Some(s.as_str().to_string());
        }
        None
    }

    /// Extracts optional custom name tag.
    pub(crate) fn parse_custom_name(comp: &NbtCompound) -> Option<String> {
        comp.string("CustomName")
            .or_else(|| comp.string("custom_name"))
            .map(|s| s.as_str().to_string())
    }

    /// Extracts continuous 3D world position from Pos tag.
    pub(crate) fn parse_pos(comp: &NbtCompound) -> Option<DVec3> {
        let pos_list = comp.list("Pos").or_else(|| comp.list("pos"))?;
        if let Some(doubles) = pos_list.doubles() {
            if doubles.len() >= 3 {
                return Some(DVec3::new(doubles[0], doubles[1], doubles[2]));
            }
        }
        if let Some(floats) = pos_list.floats() {
            if floats.len() >= 3 {
                return Some(DVec3::new(
                    floats[0] as f64,
                    floats[1] as f64,
                    floats[2] as f64,
                ));
            }
        }
        None
    }

    /// Converts a Minecraft entity `[yaw, pitch]` pair (degrees) into the canonical
    /// local-model Euler rotation `(pitch, yaw, roll)` in radians.
    ///
    /// Minecraft yaw 0 points South, while the canonical model-space yaw 0 points
    /// North (local model front is -Z). Positive Minecraft pitch looks down, so the
    /// model-space X rotation is negated.
    pub(crate) fn mc_rotation_to_model_euler(yaw_deg: f32, pitch_deg: f32) -> Vec3 {
        let mut model_yaw = std::f32::consts::PI - yaw_deg.to_radians();
        while model_yaw > std::f32::consts::PI {
            model_yaw -= std::f32::consts::TAU;
        }
        while model_yaw <= -std::f32::consts::PI {
            model_yaw += std::f32::consts::TAU;
        }
        let model_pitch = -pitch_deg.to_radians();
        Vec3::new(model_pitch, model_yaw, 0.0)
    }

    /// Extracts Euler rotation (pitch, yaw, roll) in radians from Rotation tag.
    pub(crate) fn parse_rotation_euler(comp: &NbtCompound) -> Vec3 {
        if let Some(rot_list) = comp.list("Rotation").or_else(|| comp.list("rotation")) {
            if let Some(floats) = rot_list.floats() {
                if floats.len() >= 2 {
                    return Self::mc_rotation_to_model_euler(floats[0], floats[1]);
                }
            }
            if let Some(doubles) = rot_list.doubles() {
                if doubles.len() >= 2 {
                    return Self::mc_rotation_to_model_euler(doubles[0] as f32, doubles[1] as f32);
                }
            }
        }
        Vec3::ZERO
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
                .map(|d| (d[0], d[1], d[2]))
                .or_else(|| {
                    pos_list
                        .floats()
                        .filter(|f| f.len() == 3)
                        .map(|f| (f[0] as f64, f[1] as f64, f[2] as f64))
                });

            coords.map(|(px, py, pz)| {
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
                let anchor_x = (px - 0.5 + (facing_norm.x as f64) * 0.46875
                    - (left_norm.x as f64) * h_off)
                    .round() as i32;
                let anchor_y = (py - 0.5 + (facing_norm.y as f64) * 0.46875 - v_off).round() as i32;
                let anchor_z = (pz - 0.5 + (facing_norm.z as f64) * 0.46875
                    - (left_norm.z as f64) * h_off)
                    .round() as i32;
                IVec3::new(anchor_x, anchor_y, anchor_z)
            })
        } else {
            None
        }?;

        // 4. Continuous position & 3D Bounding box intersection check
        let exact_pos = Self::parse_pos(comp);
        let center = exact_pos.unwrap_or_else(|| {
            mtk_model::painting::calculate_painting_world_position(
                [block_pos.x, block_pos.y, block_pos.z],
                facing,
                width,
                height,
            )
            .as_dvec3()
        });
        let h_extent = (width as f64) * 0.5;
        let v_extent = (height as f64) * 0.5;
        let (half_x, half_z) = match facing {
            Direction::East | Direction::West => (0.03125f64, h_extent),
            Direction::North | Direction::South => (h_extent, 0.03125f64),
            _ => (h_extent, h_extent),
        };
        let p_min_x = center.x - half_x;
        let p_max_x = center.x + half_x;
        let p_min_y = center.y - v_extent;
        let p_max_y = center.y + v_extent;
        let p_min_z = center.z - half_z;
        let p_max_z = center.z + half_z;

        let b_min_x = min_block.x as f64;
        let b_max_x = (max_block.x + 1) as f64;
        let b_min_y = min_block.y as f64;
        let b_max_y = (max_block.y + 1) as f64;
        let b_min_z = min_block.z as f64;
        let b_max_z = (max_block.z + 1) as f64;

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

        let (calculated_pos, rot_euler) = mtk_model::painting::calculate_painting_world_transform(
            [block_pos.x, block_pos.y, block_pos.z],
            facing,
            width,
            height,
        );
        let world_pos = exact_pos.unwrap_or(calculated_pos);

        let uuid = Self::parse_uuid(comp);
        let custom_name = Self::parse_custom_name(comp);

        let mut inst = EntityInstance::new_painting(
            uuid,
            world_pos,
            facing,
            rot_euler,
            variant_slug,
            width,
            height,
            Some([block_pos.x, block_pos.y, block_pos.z]),
        );
        inst.custom_name = custom_name;
        Some(inst)
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

        // 2. Position (Preserves exact float coordinates)
        let pos = if let Some(pos) = Self::parse_pos(comp) {
            pos
        } else {
            let tile_x = comp.int("TileX")?;
            let tile_y = comp.int("TileY").unwrap_or(0);
            let tile_z = comp.int("TileZ").unwrap_or(0);
            DVec3::new(
                tile_x as f64 + 0.5,
                tile_y as f64 + 0.5,
                tile_z as f64 + 0.5,
            )
        };

        let block_x = pos.x.floor() as i32;
        let block_y = pos.y.floor() as i32;
        let block_z = pos.z.floor() as i32;

        if block_x < min_block.x
            || block_x > max_block.x
            || block_y < min_block.y
            || block_y > max_block.y
            || block_z < min_block.z
            || block_z > max_block.z
        {
            return None;
        }

        // 3. Item payload
        let item_comp = comp.compound("Item").or_else(|| comp.compound("item"));
        let item_id = item_comp
            .and_then(|ic| ic.string("id").or_else(|| ic.string("Id")))
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        let item_count = item_comp
            .and_then(|ic| {
                ic.byte("Count")
                    .or_else(|| ic.byte("count"))
                    .map(|b| b as u8)
                    .or_else(|| ic.int("count").map(|i| i as u8))
            })
            .unwrap_or(1);

        let map_id = item_comp.and_then(|ic| {
            // 1.20.5+ data components
            if let Some(comp_map) = ic.compound("components") {
                if let Some(mid) = comp_map
                    .int("minecraft:map_id")
                    .or_else(|| comp_map.int("map_id"))
                {
                    return Some(mid);
                }
            }
            // Pre-1.20.5 tag compound
            if let Some(tag_map) = ic.compound("tag") {
                if let Some(mid) = tag_map.int("map").or_else(|| tag_map.int("map_id")) {
                    return Some(mid);
                }
            }
            // Direct map tag on item
            ic.int("map_id").or_else(|| ic.int("map"))
        });

        let item_rotation = comp
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

        let uuid = Self::parse_uuid(comp);
        let custom_name = Self::parse_custom_name(comp);
        let rot_euler = if comp
            .list("Rotation")
            .or_else(|| comp.list("rotation"))
            .is_some()
        {
            Self::parse_rotation_euler(comp)
        } else if matches!(
            facing,
            Direction::North | Direction::South | Direction::East | Direction::West
        ) {
            Vec3::new(0.0, mtk_model::painting::facing_to_y_rot(facing), 0.0)
        } else {
            Vec3::ZERO
        };

        let mut inst = EntityInstance::new_item_frame(
            uuid,
            pos,
            facing,
            rot_euler,
            item_id,
            item_count,
            item_rotation,
            is_glowing,
            is_invisible,
            map_id,
        );
        inst.custom_name = custom_name;
        Some(inst)
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
        match entity.kind {
            EntityKindData::Painting(p) => {
                assert_eq!(p.variant, "unpacked");
                assert_eq!(p.width, 4);
                assert_eq!(p.height, 4);
                assert_eq!(p.anchor_block, Some([-8, -59, 10]));
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
        match entity.kind {
            EntityKindData::Painting(p) => {
                assert_eq!(p.variant, "kebab");
                assert_eq!(p.width, 1);
                assert_eq!(p.height, 1);
                assert_eq!(p.anchor_block, Some([10, 64, 20]));
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
        match entity.kind {
            EntityKindData::Painting(p) => {
                assert_eq!(p.variant, "pool");
                assert_eq!(p.width, 2);
                assert_eq!(p.height, 1);
            }
            _ => panic!("Expected Painting"),
        }
    }

    #[test]
    fn test_painting_fallback_pos_reconstructs_anchor_for_all_facings() {
        let anchor = [10, 64, 20];
        let facings = [
            (0i8, Direction::South),
            (1i8, Direction::West),
            (2i8, Direction::North),
            (3i8, Direction::East),
        ];
        let dimensions = [
            (1u8, 1u8, "kebab"),
            (2, 1, "pool"),
            (1, 2, "graham"),
            (2, 2, "humble"),
            (4, 4, "pointer"),
        ];

        for &(width, height, variant) in &dimensions {
            for &(facing_byte, facing) in &facings {
                let (pos, _) = mtk_model::painting::calculate_painting_world_transform(
                    anchor, facing, width, height,
                );

                let mut comp = NbtCompound::new();
                comp.entries.push((
                    NbtStr::from_borrowed("id"),
                    NbtTag::String(NbtStr::from_borrowed("painting")),
                ));
                comp.entries.push((
                    NbtStr::from_borrowed("variant"),
                    NbtTag::String(NbtStr::from_borrowed(variant)),
                ));
                comp.entries
                    .push((NbtStr::from_borrowed("facing"), NbtTag::Byte(facing_byte)));
                comp.entries.push((
                    NbtStr::from_borrowed("Pos"),
                    NbtTag::List(NbtList::Double(vec![pos.x, pos.y, pos.z])),
                ));

                let entity = EntityParser::parse_entity_compound(
                    &comp,
                    IVec3::new(-100, -100, -100),
                    IVec3::new(200, 300, 200),
                )
                .unwrap_or_else(|| {
                    panic!("Should parse fallback painting {width}x{height} facing {facing:?}")
                });

                let EntityKindData::Painting(p) = entity.kind else {
                    panic!("Expected Painting");
                };
                assert_eq!(
                    p.anchor_block,
                    Some(anchor),
                    "wrong anchor for {width}x{height} {facing:?}"
                );
                assert_eq!(entity.facing, Some(facing));
                assert_eq!(entity.position, pos);
            }
        }
    }

    #[test]
    fn test_mc_rotation_to_model_euler_converts_yaw_and_pitch() {
        let cases = [
            (0.0f32, 0.0f32, std::f32::consts::PI),     // South -> PI
            (90.0, 0.0, std::f32::consts::FRAC_PI_2),   // West -> PI/2
            (180.0, 0.0, 0.0),                          // North -> 0.0
            (270.0, 0.0, -std::f32::consts::FRAC_PI_2), // East -> -PI/2
            (-90.0, 0.0, -std::f32::consts::FRAC_PI_2), // East -> -PI/2
            (360.0, 0.0, std::f32::consts::PI),         // South -> PI
        ];

        for (yaw_deg, pitch_deg, expected_yaw) in cases {
            let rot = EntityParser::mc_rotation_to_model_euler(yaw_deg, pitch_deg);
            assert!((rot.x - 0.0).abs() < 1e-5);
            assert!(
                (rot.y - expected_yaw).abs() < 1e-5,
                "yaw {yaw_deg} got {}, expected {expected_yaw}",
                rot.y
            );
            assert!((rot.z - 0.0).abs() < 1e-5);
        }

        let pitched = EntityParser::mc_rotation_to_model_euler(90.0, 30.0);
        assert!((pitched.x + 30.0f32.to_radians()).abs() < 1e-5);
        assert!((pitched.y - std::f32::consts::FRAC_PI_2).abs() < 1e-5);
    }

    #[test]
    fn test_item_frame_rotation_uses_model_space_yaw() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:item_frame")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("Pos"),
            NbtTag::List(NbtList::Double(vec![10.5, 64.5, 20.5])),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("Facing"), NbtTag::Byte(2))); // North
        comp.entries.push((
            NbtStr::from_borrowed("Rotation"),
            NbtTag::List(NbtList::Float(vec![180.0, 0.0])),
        ));

        let entity = EntityParser::parse_entity_compound(
            &comp,
            IVec3::new(0, 0, 0),
            IVec3::new(32, 128, 32),
        )
        .expect("Should parse item frame");

        assert_eq!(entity.facing, Some(Direction::North));
        assert!(entity.rotation.y.abs() < 1e-5);
        assert!(entity.rotation.x.abs() < 1e-5);
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

    #[test]
    fn test_parse_entity_compound_case_insensitive() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("Minecraft:Painting")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("variant"),
            NbtTag::String(NbtStr::from_borrowed("kebab")),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("facing"), NbtTag::Byte(0)));
        let bp_bytes = [5i32.to_be_bytes(), 64i32.to_be_bytes(), 5i32.to_be_bytes()].concat();
        comp.entries.push((
            NbtStr::from_borrowed("block_pos"),
            NbtTag::IntArray(&bp_bytes),
        ));

        let entity = EntityParser::parse_entity_compound(
            &comp,
            IVec3::new(0, 0, 0),
            IVec3::new(16, 128, 16),
        )
        .expect("Should parse case-insensitive Minecraft:Painting");
        assert_eq!(entity.id, "minecraft:painting");
    }

    #[test]
    fn test_parse_item_frame_with_count_and_map_id() {
        let mut comp = NbtCompound::new();
        comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:item_frame")),
        ));
        comp.entries.push((
            NbtStr::from_borrowed("Pos"),
            NbtTag::List(NbtList::Double(vec![10.5, 64.5, 20.5])),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("Facing"), NbtTag::Byte(2))); // North

        let mut item = NbtCompound::new();
        item.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:filled_map")),
        ));
        item.entries
            .push((NbtStr::from_borrowed("Count"), NbtTag::Byte(3)));
        let mut components = NbtCompound::new();
        components
            .entries
            .push((NbtStr::from_borrowed("minecraft:map_id"), NbtTag::Int(42)));
        item.entries.push((
            NbtStr::from_borrowed("components"),
            NbtTag::Compound(components),
        ));
        comp.entries
            .push((NbtStr::from_borrowed("Item"), NbtTag::Compound(item)));

        let entity = EntityParser::parse_entity_compound(
            &comp,
            IVec3::new(0, 0, 0),
            IVec3::new(32, 128, 32),
        )
        .expect("Should parse item frame with count and map_id");

        match entity.kind {
            EntityKindData::ItemFrame(f) => {
                assert_eq!(f.item_id, "minecraft:filled_map");
                assert_eq!(f.count, 3);
                assert_eq!(f.map_id, Some(42));
            }
            _ => panic!("Expected ItemFrame kind"),
        }
    }
}
