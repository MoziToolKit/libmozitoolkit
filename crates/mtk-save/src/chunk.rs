use crate::error::SaveError;
use crate::nbt::{Nbt, NbtCompound};
use glam::IVec3;
use mtk_core::constants::voxel::{block_index, SECTION_VOLUME};
use mtk_voxel::SectionStorage;
use std::io::Cursor;

pub(crate) use mtk_voxel::SectionBiomes;

/// Formats a blockstate and its properties into a canonical sorted, DataVersion-upgraded string.
///
/// Example: `minecraft:oak_stairs[facing=north,half=bottom,waterlogged=false]`
pub(crate) fn format_canonical_blockstate(block_comp: &NbtCompound, data_version: i32) -> String {
    let name = block_comp
        .string("Name")
        .map(|s| s.to_string_lossy())
        .unwrap_or_else(|| "minecraft:air".into());

    let mut prop_list: Vec<(String, String)> = Vec::new();
    if let Some(props) = block_comp.compound("Properties") {
        for (k, v) in props.iter() {
            let key_str = k.to_string_lossy().into_owned();
            let val_str = v
                .string()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            prop_list.push((key_str, val_str));
        }
    }

    let upgraded_name =
        crate::upgrade::upgrade_block_state(name.as_ref(), &mut prop_list, data_version);
    if upgraded_name == "minecraft:air" {
        return "minecraft:air".to_string();
    }

    if !prop_list.is_empty() {
        prop_list.sort_by(|a, b| a.0.cmp(&b.0));
        let prop_str = prop_list
            .into_iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        return format!("{upgraded_name}[{prop_str}]");
    }

    upgraded_name
}

/// Parses a Minecraft 1.13+ chunk and yields populated `SectionStorage` instances.
pub(crate) struct ChunkParser;

impl ChunkParser {
    /// Parses a single section from a 1.18+ (`block_states`) or 1.13–1.17 (`Palette`/`BlockStates`) section tag.
    ///
    /// Returns `Ok(None)` if the section is entirely air or unpopulated.
    pub(crate) fn parse_section(
        chunk_coord: IVec3,
        section_comp: &NbtCompound,
        data_version: i32,
    ) -> Result<Option<SectionStorage>, SaveError> {
        let sec_y = Self::read_section_y(section_comp).ok_or_else(|| {
            SaveError::InvalidChunkData("Section missing 'Y' coordinate".to_string())
        })?;

        let (palette_compounds, data_longs) =
            if let Some(bs) = section_comp.compound("block_states") {
                (
                    bs.list("palette").and_then(|p| p.compounds()),
                    bs.long_array("data"),
                )
            } else {
                (
                    section_comp.list("Palette").and_then(|p| p.compounds()),
                    section_comp.long_array("BlockStates"),
                )
            };

        let Some(palette_compounds) = palette_compounds else {
            return Ok(None);
        };

        if palette_compounds.is_empty() {
            return Ok(None);
        }

        // 1. Build palette of canonical upgraded blockstate strings
        let mut palette: Vec<String> = Vec::with_capacity(palette_compounds.len());
        let mut all_air = true;
        for b in palette_compounds {
            let state = format_canonical_blockstate(&b, data_version);
            if state != "minecraft:air" {
                all_air = false;
            }
            palette.push(state);
        }

        // Fast skip if the entire section palette contains only air
        if all_air {
            return Ok(None);
        }

        let section_world_coord = IVec3::new(chunk_coord.x, sec_y, chunk_coord.z);
        let mut section = SectionStorage::new(section_world_coord);

        if palette.len() == 1 || data_longs.is_none() {
            // Entire 16x16x16 section is filled with a single state
            if let Some(single_state) = palette.first() {
                let pal_id = 1u16;
                section.palette.push(single_state.clone());
                section.voxels.fill(pal_id);
                section.non_air_count = SECTION_VOLUME as u32;
                return Ok(Some(section));
            }
            return Ok(None);
        }

        let Some(data) = data_longs else {
            return Ok(Some(section));
        };
        let min_bits =
            (32 - ((palette.len() as u32).saturating_sub(1)).leading_zeros()).max(4) as usize;
        let aligned = if data_version > 0 {
            data_version >= 2527
        } else {
            data.len() >= SECTION_VOLUME.div_ceil(64 / min_bits)
        };
        let bits_per_block = Self::resolve_bits_per_block(min_bits, data.len(), aligned);
        let blocks_per_long = 64 / bits_per_block;
        let mask = (1u64 << bits_per_block) - 1;

        // Map section palette to SectionStorage palette (slot 0 is always air)
        let mut pal_map: Vec<u16> = Vec::with_capacity(palette.len());
        for st in &palette {
            if st == "minecraft:air" {
                pal_map.push(0);
            } else {
                let id = section.palette.len() as u16;
                section.palette.push(st.clone());
                pal_map.push(id);
            }
        }

        let mut non_air = 0u32;

        for i in 0..SECTION_VOLUME {
            let raw_pal_idx = if aligned {
                let long_idx = i / blocks_per_long;
                if long_idx >= data.len() {
                    break;
                }
                let bit_offset = (i % blocks_per_long) * bits_per_block;
                ((data[long_idx] as u64 >> bit_offset) & mask) as usize
            } else {
                // 1.13–1.15 (< 2527): packed tightly across 64-bit long boundaries
                let bit_pos = i * bits_per_block;
                let start_long = bit_pos >> 6;
                if start_long >= data.len() {
                    break;
                }
                let start_bit = bit_pos & 63;
                let end_long = ((i + 1) * bits_per_block - 1) >> 6;
                let raw = if start_long == end_long {
                    (data[start_long] as u64) >> start_bit
                } else {
                    let high = if end_long < data.len() {
                        (data[end_long] as u64) << (64 - start_bit)
                    } else {
                        0
                    };
                    ((data[start_long] as u64) >> start_bit) | high
                };
                (raw & mask) as usize
            };

            if raw_pal_idx < pal_map.len() {
                let target_pal_id = pal_map[raw_pal_idx];
                if target_pal_id != 0 {
                    // Minecraft chunk layout: i = (y << 8) | (z << 4) | x
                    let lx = i & 15;
                    let lz = (i >> 4) & 15;
                    let ly = (i >> 8) & 15;

                    // mtk-core internal coordinate index: block_index(x, y, z)
                    let internal_idx = block_index(lx, ly, lz);
                    section.voxels[internal_idx] = target_pal_id;
                    non_air += 1;
                }
            }
        }

        section.non_air_count = non_air;

        if non_air > 0 {
            Ok(Some(section))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn read_section_y(section_comp: &NbtCompound) -> Option<i32> {
        section_comp
            .byte("Y")
            .map(|b| b as i32)
            .or_else(|| section_comp.int("Y"))
    }

    /// Infers the actual bits-per-block used in `data` (handling uncompacted 1.13–1.17 palettes).
    fn resolve_bits_per_block(min_bits: usize, data_len: usize, aligned: bool) -> usize {
        if aligned {
            (min_bits..=12)
                .find(|&b| SECTION_VOLUME.div_ceil(64 / b) == data_len)
                .unwrap_or(min_bits)
        } else {
            let inferred = (data_len * 64) / SECTION_VOLUME;
            let clamped = inferred.clamp(min_bits, 12);
            if (SECTION_VOLUME * clamped).div_ceil(64) == data_len {
                clamped
            } else {
                min_bits
            }
        }
    }

    /// Parses the 1.18+ `biomes` paletted container from a section compound tag.
    pub(crate) fn parse_biomes(
        section_comp: &NbtCompound,
        data_version: i32,
    ) -> Option<SectionBiomes> {
        let biomes_comp = section_comp.compound("biomes")?;
        let pal_strings = biomes_comp.list("palette").and_then(|l| l.strings())?;
        if pal_strings.is_empty() {
            return None;
        }

        let palette: Vec<String> = pal_strings
            .iter()
            .map(|s| crate::upgrade::upgrade_biome_name(s.as_str(), data_version))
            .collect();

        if palette.len() <= 1 {
            return Some(SectionBiomes::new(palette, Vec::new()));
        }

        let pal_len = palette.len() as u32;
        let bits_per_biome = (32 - (pal_len.saturating_sub(1)).leading_zeros()).max(1) as usize;
        let entries_per_long = 64 / bits_per_biome;
        let mask = (1u64 << bits_per_biome) - 1;

        let data_longs = match biomes_comp.long_array("data") {
            Some(d) => d,
            None => return Some(SectionBiomes::new(palette, Vec::new())),
        };

        let mut indices = Vec::with_capacity(64);
        for i in 0..64 {
            let long_idx = i / entries_per_long;
            if long_idx >= data_longs.len() {
                break;
            }
            let bit_offset = (i % entries_per_long) * bits_per_biome;
            let raw_pal_idx = ((data_longs[long_idx] as u64 >> bit_offset) & mask) as u8;
            indices.push(raw_pal_idx);
        }

        Some(SectionBiomes::new(palette, indices))
    }

    /// Parses 1.13–1.17 numeric chunk `Biomes` (`IntArray(1024)` 3D or `IntArray(256)` 2D) for `sec_y`.
    fn parse_legacy_section_biomes(chunk_biomes: &[i32], sec_y: i32) -> Option<SectionBiomes> {
        let mut palette: Vec<String> = Vec::new();
        let mut indices: Vec<u8> = Vec::with_capacity(64);

        let mut push_id = |id: i32| {
            let name = crate::upgrade::biome_name_from_legacy_id(id);
            let pal_idx = if let Some(pos) = palette.iter().position(|p| p == name) {
                pos as u8
            } else {
                let pos = palette.len() as u8;
                palette.push(name.to_string());
                pos
            };
            indices.push(pal_idx);
        };

        if chunk_biomes.len() == 1024 {
            if (0..16).contains(&sec_y) {
                let base = (sec_y as usize) * 64;
                for &id in &chunk_biomes[base..base + 64] {
                    push_id(id);
                }
            } else {
                let layer_base = if sec_y < 0 { 0 } else { 1008 };
                for i in 0..64 {
                    push_id(chunk_biomes[layer_base + (i % 16)]);
                }
            }
        } else if chunk_biomes.len() == 256 {
            for _qy in 0..4 {
                for qz in 0..4 {
                    for qx in 0..4 {
                        let old_x = (qx << 2) + 2;
                        let old_z = (qz << 2) + 2;
                        push_id(chunk_biomes[(old_z << 4) | old_x]);
                    }
                }
            }
        } else {
            return None;
        }

        if palette.len() <= 1 {
            Some(SectionBiomes::new(palette, Vec::new()))
        } else {
            Some(SectionBiomes::new(palette, indices))
        }
    }

    /// Parses decompressed chunk bytes, querying sections within `min_section_y..=max_section_y`,
    /// returning populated sections.
    pub(crate) fn parse_chunk(
        decompressed_bytes: &[u8],
        min_section_y: i32,
        max_section_y: i32,
    ) -> Result<Vec<SectionStorage>, SaveError> {
        let nbt = crate::nbt::read(&mut Cursor::new(decompressed_bytes))?;
        let base = match nbt {
            Nbt::Some(b) => b,
            Nbt::None => return Ok(Vec::new()),
        };

        let root = base.as_compound();
        let data_version = root.int("DataVersion").unwrap_or(0);

        // Modern 1.18+ chunks place `sections` at root; 1.13–1.17.1 chunks wrap data in `Level`.
        let (chunk_body, sections_list, be_list, legacy_biomes) = if let Some(modern_sections) =
            root.list("sections").and_then(|l| l.compounds())
        {
            (
                root,
                modern_sections,
                root.list("block_entities").and_then(|l| l.compounds()),
                None,
            )
        } else if let Some(level) = root.compound("Level") {
            let Some(legacy_sections) = level.list("Sections").and_then(|l| l.compounds()) else {
                return Ok(Vec::new());
            };
            (
                level,
                legacy_sections,
                level.list("TileEntities").and_then(|l| l.compounds()),
                level.int_array("Biomes"),
            )
        } else {
            return Ok(Vec::new());
        };

        let chunk_x = chunk_body
            .int("xPos")
            .or_else(|| chunk_body.int("x_pos"))
            .unwrap_or(0);
        let chunk_z = chunk_body
            .int("zPos")
            .or_else(|| chunk_body.int("z_pos"))
            .unwrap_or(0);
        let chunk_coord = IVec3::new(chunk_x, 0, chunk_z);

        let mut sections = Vec::new();

        for sec_comp in sections_list {
            if let Some(sec_y) = Self::read_section_y(&sec_comp) {
                if sec_y >= min_section_y && sec_y <= max_section_y {
                    let biome_data = Self::parse_biomes(&sec_comp, data_version).or_else(|| {
                        legacy_biomes
                            .as_deref()
                            .and_then(|b| Self::parse_legacy_section_biomes(b, sec_y))
                    });
                    if let Some(mut section) =
                        Self::parse_section(chunk_coord, &sec_comp, data_version)?
                    {
                        section.biomes = biome_data;
                        sections.push(section);
                    }
                }
            }
        }

        // Universal BlockEntity Decoding (Decorated Pots, Banners, Skulls, etc.)
        if let Some(be_list) = be_list {
            for be_comp in be_list {
                if let Some(entity_data) = crate::block_entity::decode_block_entity(&be_comp) {
                    let x = be_comp.int("x").unwrap_or(0);
                    let y = be_comp.int("y").unwrap_or(0);
                    let z = be_comp.int("z").unwrap_or(0);

                    // Validate chunk coordinate bounds if chunk coordinate tags are present
                    let chunk_x_valid = chunk_body
                        .int("xPos")
                        .or_else(|| chunk_body.int("x_pos"))
                        .is_none_or(|cx| x.div_euclid(16) == cx);
                    let chunk_z_valid = chunk_body
                        .int("zPos")
                        .or_else(|| chunk_body.int("z_pos"))
                        .is_none_or(|cz| z.div_euclid(16) == cz);
                    if !chunk_x_valid || !chunk_z_valid {
                        continue;
                    }

                    let sec_y = y.div_euclid(16);

                    if sec_y >= min_section_y && sec_y <= max_section_y {
                        let local_x = (x.rem_euclid(16)) as usize;
                        let local_y = (y.rem_euclid(16)) as usize;
                        let local_z = (z.rem_euclid(16)) as usize;
                        let local_idx = block_index(local_x, local_y, local_z) as u16;

                        if let Some(sec) = sections.iter_mut().find(|s| s.coord.y == sec_y) {
                            let mut entity_data = entity_data;
                            if let mtk_model::BlockEntityData::Skull {
                                ref mut kind,
                                ref profile,
                                ..
                            } = entity_data
                            {
                                if profile.is_none() {
                                    let state_str = sec.get_local_state(local_x, local_y, local_z);
                                    if let Some(resolved_kind) =
                                        mtk_model::skull::SkullKind::from_block_name(state_str)
                                    {
                                        *kind = resolved_kind;
                                    }
                                }
                            } else if let mtk_model::BlockEntityData::Banner {
                                ref mut base_color,
                                ..
                            } = entity_data
                            {
                                let state_str = sec.get_local_state(local_x, local_y, local_z);
                                if let Some(resolved_color) =
                                    mtk_model::banner::extract_banner_color_from_state(state_str)
                                {
                                    *base_color = resolved_color.to_string();
                                }
                            }
                            sec.block_entities.insert(local_idx, entity_data);
                        }
                    }
                }
            }
        }

        Ok(sections)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::{NbtList, NbtStr, NbtTag};

    #[test]
    fn test_single_biome_section() {
        let biomes = SectionBiomes::new(vec!["minecraft:desert".to_string()], Vec::new());
        assert_eq!(biomes.get_biome(0, 0, 0), Some("minecraft:desert"));
        assert_eq!(biomes.get_biome(15, 15, 15), Some("minecraft:desert"));
        assert_eq!(biomes.get_quart_biome(2, 3, 1), Some("minecraft:desert"));
    }

    #[test]
    fn test_multi_biome_section_unpacking() {
        let palette = vec![
            "minecraft:plains".to_string(),
            "minecraft:forest".to_string(),
        ];
        // 64 indices packed at 1 bit per index into 1 long (64 bits).
        // Let's set even quarts to 0 (plains), odd quarts to 1 (forest): 0b101010...
        let pattern = 0xAAAAAAAAAAAAAAAAu64 as i64;
        let mut comp = NbtCompound::new();
        let mut pal_list = Vec::new();
        for p in &palette {
            pal_list.push(NbtStr::from_owned(p.clone()));
        }
        comp.entries.push((
            NbtStr::from_borrowed("palette"),
            NbtTag::List(NbtList::String(pal_list)),
        ));

        let be_bytes = pattern.to_be_bytes();
        comp.entries
            .push((NbtStr::from_borrowed("data"), NbtTag::LongArray(&be_bytes)));

        let mut sec_comp = NbtCompound::new();
        sec_comp
            .entries
            .push((NbtStr::from_borrowed("biomes"), NbtTag::Compound(comp)));

        let parsed = ChunkParser::parse_biomes(&sec_comp, 0).expect("Should parse biomes");
        assert_eq!(parsed.palette.len(), 2);
        assert_eq!(parsed.indices.len(), 64);

        for i in 0..64 {
            let expected = if i % 2 == 0 {
                "minecraft:plains"
            } else {
                "minecraft:forest"
            };
            let qx = i & 3;
            let qz = (i >> 2) & 3;
            let qy = (i >> 4) & 3;
            assert_eq!(parsed.get_quart_biome(qx, qy, qz), Some(expected));
        }
    }

    #[test]
    fn test_parse_chunk_with_decorated_pot() {
        use crate::nbt::*;
        let mut buf = Vec::new();
        // Root compound
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&0u16.to_be_bytes()); // empty name

        // int xPos = 0
        buf.push(TAG_INT);
        buf.extend_from_slice(&(4u16).to_be_bytes());
        buf.extend_from_slice(b"xPos");
        buf.extend_from_slice(&0i32.to_be_bytes());

        // int zPos = 0
        buf.push(TAG_INT);
        buf.extend_from_slice(&(4u16).to_be_bytes());
        buf.extend_from_slice(b"zPos");
        buf.extend_from_slice(&0i32.to_be_bytes());

        // sections list: [ compound with Y=0, block_states with single-element palette "minecraft:decorated_pot" ]
        buf.push(TAG_LIST);
        buf.extend_from_slice(&(8u16).to_be_bytes());
        buf.extend_from_slice(b"sections");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes()); // 1 section

        // Section 0
        // byte Y = 0
        buf.push(TAG_BYTE);
        buf.extend_from_slice(&(1u16).to_be_bytes());
        buf.extend_from_slice(b"Y");
        buf.push(0);

        // compound block_states
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&(12u16).to_be_bytes());
        buf.extend_from_slice(b"block_states");

        // list palette of compounds
        buf.push(TAG_LIST);
        buf.extend_from_slice(&(7u16).to_be_bytes());
        buf.extend_from_slice(b"palette");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes()); // 1 palette entry

        // Palette entry 0: string Name = "minecraft:decorated_pot"
        buf.push(TAG_STRING);
        buf.extend_from_slice(&(4u16).to_be_bytes());
        buf.extend_from_slice(b"Name");
        let pot_name = b"minecraft:decorated_pot";
        buf.extend_from_slice(&(pot_name.len() as u16).to_be_bytes());
        buf.extend_from_slice(pot_name);
        buf.push(TAG_END); // end of palette entry 0

        buf.push(TAG_END); // end of block_states compound
        buf.push(TAG_END); // end of section 0 compound

        // block_entities list: 1 compound
        buf.push(TAG_LIST);
        buf.extend_from_slice(&(14u16).to_be_bytes());
        buf.extend_from_slice(b"block_entities");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes()); // 1 block entity

        // Block Entity 0
        // string id = "minecraft:decorated_pot"
        buf.push(TAG_STRING);
        buf.extend_from_slice(&(2u16).to_be_bytes());
        buf.extend_from_slice(b"id");
        buf.extend_from_slice(&(pot_name.len() as u16).to_be_bytes());
        buf.extend_from_slice(pot_name);

        // int x = 3
        buf.push(TAG_INT);
        buf.extend_from_slice(&(1u16).to_be_bytes());
        buf.extend_from_slice(b"x");
        buf.extend_from_slice(&3i32.to_be_bytes());

        // int y = 2
        buf.push(TAG_INT);
        buf.extend_from_slice(&(1u16).to_be_bytes());
        buf.extend_from_slice(b"y");
        buf.extend_from_slice(&2i32.to_be_bytes());

        // int z = 5
        buf.push(TAG_INT);
        buf.extend_from_slice(&(1u16).to_be_bytes());
        buf.extend_from_slice(b"z");
        buf.extend_from_slice(&5i32.to_be_bytes());

        // list sherds: [angler, brick, miner, blade]
        buf.push(TAG_LIST);
        buf.extend_from_slice(&(6u16).to_be_bytes());
        buf.extend_from_slice(b"sherds");
        buf.push(TAG_STRING);
        buf.extend_from_slice(&4i32.to_be_bytes());
        let sherds: [&[u8]; 4] = [
            b"minecraft:angler_pottery_sherd",
            b"minecraft:brick",
            b"miner_pottery_sherd",
            b"minecraft:blade_pottery_sherd",
        ];
        for s in sherds {
            buf.extend_from_slice(&(s.len() as u16).to_be_bytes());
            buf.extend_from_slice(s);
        }

        buf.push(TAG_END); // end of block entity 0
        buf.push(TAG_END); // end of root compound

        let sections = ChunkParser::parse_chunk(&buf, -4, 20).expect("Should parse chunk");
        assert_eq!(sections.len(), 1);
        let sec = &sections[0];
        assert_eq!(sec.coord.y, 0);

        let local_idx = block_index(3, 2, 5) as u16;
        let be = sec
            .get_block_entity(local_idx)
            .expect("Must contain block entity");
        match be {
            mtk_voxel::BlockEntityData::DecoratedPot { sherds } => {
                assert_eq!(
                    sherds[0],
                    Some("minecraft:angler_pottery_sherd".to_string())
                );
                assert_eq!(sherds[1], None);
                assert_eq!(sherds[2], Some("miner_pottery_sherd".to_string()));
                assert_eq!(sherds[3], Some("minecraft:blade_pottery_sherd".to_string()));
            }
            _ => panic!("Expected DecoratedPot"),
        }
    }

    #[test]
    fn test_parse_block_states_missing_data_longs_and_empty_palette() {
        let chunk_coord = glam::IVec3::new(0, 0, 0);

        // Case 1: Palette with 1 block, missing data_longs (fills entire section)
        let mut bs_compound = NbtCompound::new();
        let mut pal_list = Vec::new();
        let mut stone_comp = NbtCompound::new();
        stone_comp.entries.push((
            NbtStr::from_borrowed("Name"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:stone")),
        ));
        pal_list.push(stone_comp);
        bs_compound.entries.push((
            NbtStr::from_borrowed("palette"),
            NbtTag::List(NbtList::Compound(pal_list)),
        ));

        let mut sec_comp = NbtCompound::new();
        sec_comp
            .entries
            .push((NbtStr::from_borrowed("Y"), NbtTag::Byte(0)));
        sec_comp.entries.push((
            NbtStr::from_borrowed("block_states"),
            NbtTag::Compound(bs_compound),
        ));

        let parsed = ChunkParser::parse_section(chunk_coord, &sec_comp, 0).expect("parse section");
        assert!(parsed.is_some());
        let sec = parsed.unwrap();
        assert_eq!(sec.palette.len(), 2); // air + stone
        assert_eq!(sec.non_air_count, SECTION_VOLUME as u32);

        // Case 2: Empty palette compounds -> returns Ok(None) without panic
        let mut empty_bs = NbtCompound::new();
        empty_bs.entries.push((
            NbtStr::from_borrowed("palette"),
            NbtTag::List(NbtList::Compound(Vec::new())),
        ));
        let mut sec_comp_empty = NbtCompound::new();
        sec_comp_empty
            .entries
            .push((NbtStr::from_borrowed("Y"), NbtTag::Byte(0)));
        sec_comp_empty.entries.push((
            NbtStr::from_borrowed("block_states"),
            NbtTag::Compound(empty_bs),
        ));
        let parsed_empty =
            ChunkParser::parse_section(chunk_coord, &sec_comp_empty, 0).expect("parse empty");
        assert!(parsed_empty.is_none());

        // Case 3: Multi-state palette with MISSING data_longs -> returns Ok(Some(sec)) without panic
        let mut multi_bs = NbtCompound::new();
        let mut multi_pal = Vec::new();
        let mut dirt_comp = NbtCompound::new();
        dirt_comp.entries.push((
            NbtStr::from_borrowed("Name"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:dirt")),
        ));
        multi_pal.push(dirt_comp);
        let mut grass_comp = NbtCompound::new();
        grass_comp.entries.push((
            NbtStr::from_borrowed("Name"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:grass_block")),
        ));
        multi_pal.push(grass_comp);
        multi_bs.entries.push((
            NbtStr::from_borrowed("palette"),
            NbtTag::List(NbtList::Compound(multi_pal)),
        ));
        let mut sec_comp_multi = NbtCompound::new();
        sec_comp_multi
            .entries
            .push((NbtStr::from_borrowed("Y"), NbtTag::Byte(0)));
        sec_comp_multi.entries.push((
            NbtStr::from_borrowed("block_states"),
            NbtTag::Compound(multi_bs),
        ));
        let parsed_multi =
            ChunkParser::parse_section(chunk_coord, &sec_comp_multi, 0).expect("parse multi");
        assert!(parsed_multi.is_some());
    }

    fn push_nbt_header(buf: &mut Vec<u8>, tag_type: u8, name: &str) {
        buf.push(tag_type);
        buf.extend_from_slice(&(name.len() as u16).to_be_bytes());
        buf.extend_from_slice(name.as_bytes());
    }

    fn push_nbt_int(buf: &mut Vec<u8>, name: &str, value: i32) {
        push_nbt_header(buf, crate::nbt::TAG_INT, name);
        buf.extend_from_slice(&value.to_be_bytes());
    }

    fn push_nbt_string(buf: &mut Vec<u8>, name: &str, value: &str) {
        push_nbt_header(buf, crate::nbt::TAG_STRING, name);
        buf.extend_from_slice(&(value.len() as u16).to_be_bytes());
        buf.extend_from_slice(value.as_bytes());
    }

    fn push_nbt_int_array(buf: &mut Vec<u8>, name: &str, values: &[i32]) {
        push_nbt_header(buf, crate::nbt::TAG_INT_ARRAY, name);
        buf.extend_from_slice(&(values.len() as i32).to_be_bytes());
        for &v in values {
            buf.extend_from_slice(&v.to_be_bytes());
        }
    }

    fn push_nbt_long_array(buf: &mut Vec<u8>, name: &str, values: &[u64]) {
        push_nbt_header(buf, crate::nbt::TAG_LONG_ARRAY, name);
        buf.extend_from_slice(&(values.len() as i32).to_be_bytes());
        for &v in values {
            buf.extend_from_slice(&(v as i64).to_be_bytes());
        }
    }

    #[test]
    fn test_parse_1_17_level_chunk_with_banner_and_upgrade() {
        use crate::nbt::*;
        let mut buf = Vec::new();
        push_nbt_header(&mut buf, TAG_COMPOUND, "");
        push_nbt_int(&mut buf, "DataVersion", 2724);

        push_nbt_header(&mut buf, TAG_COMPOUND, "Level");
        push_nbt_int(&mut buf, "xPos", 0);
        push_nbt_int(&mut buf, "zPos", 0);

        let mut biomes = vec![1i32; 1024];
        biomes[0..64].fill(12); // snowy_tundra -> snowy_plains
        push_nbt_int_array(&mut buf, "Biomes", &biomes);

        push_nbt_header(&mut buf, TAG_LIST, "Sections");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes());
        push_nbt_header(&mut buf, TAG_BYTE, "Y");
        buf.push(0);

        push_nbt_header(&mut buf, TAG_LIST, "Palette");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&3i32.to_be_bytes());
        for name in ["minecraft:air", "minecraft:grass", "minecraft:red_banner"] {
            push_nbt_string(&mut buf, "Name", name);
            buf.push(TAG_END);
        }

        // Aligned 4-bit array (256 longs): index 0 = 1 (grass->short_grass), index 1 = 2 (red_banner)
        let mut longs = vec![0u64; 256];
        longs[0] = 1u64 | (2u64 << 4);
        push_nbt_long_array(&mut buf, "BlockStates", &longs);
        buf.push(TAG_END); // end Section 0

        // TileEntities: 1 banner with pre-1.20.5 integer Color
        push_nbt_header(&mut buf, TAG_LIST, "TileEntities");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes());
        push_nbt_string(&mut buf, "id", "minecraft:banner");
        push_nbt_int(&mut buf, "x", 1);
        push_nbt_int(&mut buf, "y", 0);
        push_nbt_int(&mut buf, "z", 0);
        push_nbt_header(&mut buf, TAG_LIST, "Patterns");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes());
        push_nbt_string(&mut buf, "Pattern", "cre");
        push_nbt_int(&mut buf, "Color", 14);
        buf.push(TAG_END); // pattern 0
        buf.push(TAG_END); // tile entity 0

        buf.push(TAG_END); // Level
        buf.push(TAG_END); // root

        let sections = ChunkParser::parse_chunk(&buf, 0, 15).expect("parse 1.17 chunk");
        assert_eq!(sections.len(), 1);
        let sec = &sections[0];
        assert_eq!(sec.get_local_state(0, 0, 0), "minecraft:short_grass");
        assert_eq!(sec.get_local_state(1, 0, 0), "minecraft:red_banner");

        let biomes = sec.biomes.as_ref().expect("1.17 biomes must exist");
        assert_eq!(biomes.get_biome(0, 0, 0), Some("minecraft:snowy_plains"));

        let be = sec
            .get_block_entity(block_index(1, 0, 0) as u16)
            .expect("banner must attach");
        match be {
            mtk_voxel::BlockEntityData::Banner {
                base_color,
                patterns,
            } => {
                assert_eq!(base_color, "red");
                assert_eq!(patterns.len(), 1);
                assert_eq!(patterns[0].pattern, "cre");
                assert_eq!(patterns[0].color, "red");
            }
            _ => panic!("Expected Banner"),
        }
    }

    #[test]
    fn test_parse_1_14_unaligned_level_chunk() {
        use crate::nbt::*;
        let mut buf = Vec::new();
        push_nbt_header(&mut buf, TAG_COMPOUND, "");
        push_nbt_int(&mut buf, "DataVersion", 1976); // 1.14.4 (< 2527)

        push_nbt_header(&mut buf, TAG_COMPOUND, "Level");
        push_nbt_int(&mut buf, "xPos", 0);
        push_nbt_int(&mut buf, "zPos", 0);

        let mut biomes = vec![3i32; 256]; // mountains -> windswept_hills
        biomes[(14 << 4) | 14] = 12; // snowy_tundra -> snowy_plains
        push_nbt_int_array(&mut buf, "Biomes", &biomes);

        push_nbt_header(&mut buf, TAG_LIST, "Sections");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&1i32.to_be_bytes());
        push_nbt_header(&mut buf, TAG_BYTE, "Y");
        buf.push(0);

        // 18 palette entries -> 5 bits per block (unaligned across 64-bit boundaries)
        push_nbt_header(&mut buf, TAG_LIST, "Palette");
        buf.push(TAG_COMPOUND);
        buf.extend_from_slice(&18i32.to_be_bytes());
        push_nbt_string(&mut buf, "Name", "minecraft:air");
        buf.push(TAG_END);
        for _ in 1..17 {
            push_nbt_string(&mut buf, "Name", "minecraft:stone");
            buf.push(TAG_END);
        }
        push_nbt_string(&mut buf, "Name", "minecraft:grass_path");
        buf.push(TAG_END);

        // Tightly packed 5-bit array (4096 * 5 / 64 = 320 longs).
        // Index 12 spans long[0] bits 60..64 and long[1] bit 0: store 17 (0b10001).
        // Index 13 is in long[1] bits 1..6: store 1.
        let mut longs = vec![0u64; 320];
        let set_unaligned = |arr: &mut [u64], idx: usize, val: u64| {
            let bit_pos = idx * 5;
            let start_long = bit_pos >> 6;
            let start_bit = bit_pos & 63;
            arr[start_long] |= val << start_bit;
            if start_bit + 5 > 64 {
                arr[start_long + 1] |= val >> (64 - start_bit);
            }
        };
        set_unaligned(&mut longs, 12, 17);
        set_unaligned(&mut longs, 13, 1);
        push_nbt_long_array(&mut buf, "BlockStates", &longs);
        buf.push(TAG_END); // Section 0

        buf.push(TAG_END); // Level
        buf.push(TAG_END); // root

        let sections = ChunkParser::parse_chunk(&buf, 0, 15).expect("parse 1.14 chunk");
        assert_eq!(sections.len(), 1);
        let sec = &sections[0];
        // i=12 -> lx=12, ly=0, lz=0; grass_path (<2680) -> dirt_path
        assert_eq!(sec.get_local_state(12, 0, 0), "minecraft:dirt_path");
        assert_eq!(sec.get_local_state(13, 0, 0), "minecraft:stone");
        assert_eq!(sec.get_local_state(0, 0, 0), "minecraft:air");

        let biomes = sec.biomes.as_ref().expect("1.14 biomes must exist");
        assert_eq!(
            biomes.get_quart_biome(0, 0, 0),
            Some("minecraft:windswept_hills")
        );
        assert_eq!(
            biomes.get_quart_biome(3, 3, 3),
            Some("minecraft:snowy_plains")
        );
    }
}
