//! Pre-flattening (Minecraft 1.2–1.12.2) Anvil chunk decoder.
//!
//! Converts numeric `Blocks`/`Data`/`Add` nibble arrays to modern canonical 1.13+
//! blockstate strings via the frozen Mojang `BlockStateData` lookup table and applies
//! `ChunkPalettedStorageFix` TileEntity and neighbor fixups.

use std::collections::HashMap;
use std::sync::OnceLock;

use glam::IVec3;
use mtk_core::constants::voxel::{block_index, SECTION_VOLUME};
use mtk_voxel::SectionStorage;

use crate::block_entity::{decode_block_entity, dye_color_id_to_name};
use crate::chunk::ChunkParser;
use crate::error::SaveError;
use crate::nbt::NbtCompound;
use crate::upgrade::upgrade_block_state;

const LEGACY_TABLE_TSV: &str = include_str!("legacy_blockstates.tsv");
const TABLE_LEN: usize = 4096;

/// Returns the cached 4096-entry lookup table mapping `(block_id << 4) | meta` to modern canonical states.
fn legacy_blockstate_table() -> &'static [Option<String>; TABLE_LEN] {
    static TABLE: OnceLock<Box<[Option<String>; TABLE_LEN]>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut arr: Vec<Option<String>> = vec![None; TABLE_LEN];
        for line in LEGACY_TABLE_TSV.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((idx_str, raw_state)) = trimmed.split_once('\t') {
                if let Ok(idx) = idx_str.parse::<usize>() {
                    if idx < TABLE_LEN {
                        arr[idx] = Some(upgrade_tsv_entry(raw_state));
                    }
                }
            }
        }
        arr.into_boxed_slice()
            .try_into()
            .unwrap_or_else(|_| unreachable!("table length is fixed to 4096"))
    })
}

fn upgrade_tsv_entry(raw_state: &str) -> String {
    if raw_state.starts_with("%%FILTER_ME%%") {
        return raw_state.to_string();
    }
    let (name, props_part) = match raw_state.split_once('[') {
        Some((n, rest)) => (n, Some(rest.trim_end_matches(']'))),
        None => (raw_state, None),
    };
    let mut props = Vec::new();
    if let Some(pstr) = props_part {
        for pair in pstr.split(',') {
            if let Some((k, v)) = pair.split_once('=') {
                props.push((k.to_string(), v.to_string()));
            }
        }
    }
    let upgraded_name = upgrade_block_state(name, &mut props, 1451);
    format_state(&upgraded_name, &mut props)
}

fn format_state(name: &str, props: &mut [(String, String)]) -> String {
    if props.is_empty() {
        return name.to_string();
    }
    props.sort_by(|a, b| a.0.cmp(&b.0));
    let mut out = String::with_capacity(name.len() + 32);
    out.push_str(name);
    out.push('[');
    for (idx, (k, v)) in props.iter().enumerate() {
        if idx > 0 {
            out.push(',');
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out.push(']');
    out
}

#[inline]
fn nibble(bytes: &[u8], idx: usize) -> u8 {
    let byte = bytes[idx >> 1];
    if (idx & 1) == 0 {
        byte & 0x0F
    } else {
        (byte >> 4) & 0x0F
    }
}

#[inline]
fn split_state(state: &str) -> (&str, Option<&str>) {
    match state.split_once('[') {
        Some((name, rest)) => (name, Some(rest.trim_end_matches(']'))),
        None => (state, None),
    }
}

#[inline]
fn get_prop<'a>(state: &'a str, key: &str) -> Option<&'a str> {
    let (_, Some(props)) = split_state(state) else {
        return None;
    };
    props.split(',').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then_some(v)
    })
}

fn with_prop(state: &str, key: &str, new_val: &str) -> String {
    let (name, props_opt) = split_state(state);
    let mut props: Vec<(String, String)> = props_opt
        .map(|p| {
            p.split(',')
                .filter_map(|kv| kv.split_once('='))
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        })
        .unwrap_or_default();
    if let Some((_, v)) = props.iter_mut().find(|(k, _)| k == key) {
        *v = new_val.to_string();
    } else {
        props.push((key.to_string(), new_val.to_string()));
    }
    format_state(name, &mut props)
}

struct LegacySection {
    palette: Vec<String>,
    palette_map: HashMap<String, u16>,
    voxels: Vec<u16>,
    fix_positions: Vec<(u8, u16)>,
}

impl LegacySection {
    fn new() -> Self {
        let mut palette_map = HashMap::new();
        palette_map.insert("minecraft:air".to_string(), 0);
        Self {
            palette: vec!["minecraft:air".to_string()],
            palette_map,
            voxels: vec![0u16; SECTION_VOLUME],
            fix_positions: Vec::new(),
        }
    }

    fn intern(&mut self, state: &str) -> u16 {
        if let Some(&id) = self.palette_map.get(state) {
            return id;
        }
        let id = self.palette.len() as u16;
        let owned = state.to_string();
        self.palette_map.insert(owned.clone(), id);
        self.palette.push(owned);
        id
    }

    #[inline]
    fn get_state(&self, idx: usize) -> &str {
        self.palette
            .get(self.voxels[idx] as usize)
            .map(String::as_str)
            .unwrap_or("minecraft:air")
    }

    #[inline]
    fn set_state(&mut self, idx: usize, state: &str) {
        let pal_id = self.intern(state);
        self.voxels[idx] = pal_id;
    }
}

/// Parses a pre-flattening (1.2–1.12.2) `Level` compound into modern `SectionStorage` slices.
pub(crate) fn parse_pre_flattening_chunk(
    level: &NbtCompound,
    chunk_coord: IVec3,
    min_section_y: i32,
    max_section_y: i32,
) -> Result<Vec<SectionStorage>, SaveError> {
    let table = legacy_blockstate_table();
    let sections_list = match level.list("Sections").and_then(|l| l.compounds()) {
        Some(list) => list,
        None => return Ok(Vec::new()),
    };

    let legacy_biomes: Option<Vec<i32>> = level
        .byte_array("Biomes")
        .map(|bytes| bytes.iter().map(|&b| b as i32).collect())
        .or_else(|| level.int_array("Biomes"));

    let te_list = level.list("TileEntities").and_then(|l| l.compounds());
    let mut tile_entities: HashMap<usize, NbtCompound> = HashMap::new();
    if let Some(ref list) = te_list {
        let cx_check = level.int("xPos").or_else(|| level.int("x_pos"));
        let cz_check = level.int("zPos").or_else(|| level.int("z_pos"));
        for te in list {
            let x = te.int("x").unwrap_or(0);
            let y = te.int("y").unwrap_or(0);
            let z = te.int("z").unwrap_or(0);
            if cx_check.is_some_and(|cx| x.div_euclid(16) != cx)
                || cz_check.is_some_and(|cz| z.div_euclid(16) != cz)
                || !(0..256).contains(&y)
            {
                continue;
            }
            let pos = ((y as usize) << 8)
                | ((z.rem_euclid(16) as usize) << 4)
                | (x.rem_euclid(16) as usize);
            tile_entities.insert(pos, te.clone());
        }
    }

    let mut grid: [Option<LegacySection>; 16] = std::array::from_fn(|_| None);

    for sec_comp in sections_list {
        let Some(sec_y) = sec_comp
            .byte("Y")
            .map(|b| b as i32)
            .or_else(|| sec_comp.int("Y"))
        else {
            continue;
        };
        if !(0..16).contains(&sec_y) {
            continue;
        }
        let Some(blocks) = sec_comp.byte_array("Blocks") else {
            continue;
        };
        if blocks.len() < SECTION_VOLUME {
            continue;
        }
        let data = sec_comp.byte_array("Data").filter(|d| d.len() >= 2048);
        let add = sec_comp.byte_array("Add").filter(|a| a.len() >= 2048);

        let mut sec = LegacySection::new();
        let mut table_to_pal = [u16::MAX; TABLE_LEN];
        table_to_pal[0] = 0;

        for idx in 0..SECTION_VOLUME {
            let add_nibble = add.map_or(0u16, |a| nibble(a, idx) as u16);
            let full_id = (blocks[idx] as u16) | (add_nibble << 8);
            if full_id >= 256 {
                // Pre-1.13 mod block IDs have no static vanilla state mapping.
                continue;
            }
            let raw_id = full_id as u8;
            let meta = data.map_or(0u8, |d| nibble(d, idx));
            let table_idx = ((raw_id as usize) << 4) | (meta as usize);

            if matches!(
                raw_id,
                2 | 3 | 25 | 26 | 64 | 71 | 86 | 110 | 140 | 144 | 175 | 176 | 177 | 193..=197
            ) {
                sec.fix_positions.push((raw_id, idx as u16));
            }

            let pal_id = if table_to_pal[table_idx] != u16::MAX {
                table_to_pal[table_idx]
            } else {
                let id = match table[table_idx].as_deref() {
                    Some(st) => sec.intern(st),
                    None => 0,
                };
                table_to_pal[table_idx] = id;
                id
            };
            sec.voxels[idx] = pal_id;
        }

        grid[sec_y as usize] = Some(sec);
    }

    let converted_from_alpha = level.byte("convertedFromAlphaFormat").unwrap_or(0) != 0;
    apply_post_pass_fixups(&mut grid, &tile_entities, converted_from_alpha);

    let mut out = Vec::new();
    for (sec_idx, maybe_sec) in grid.into_iter().enumerate() {
        let sec_y = sec_idx as i32;
        if sec_y < min_section_y || sec_y > max_section_y {
            continue;
        }
        let Some(sec) = maybe_sec else {
            continue;
        };

        let mut storage = SectionStorage::new(IVec3::new(chunk_coord.x, sec_y, chunk_coord.z));
        let mut pal_remap = vec![u16::MAX; sec.palette.len()];
        pal_remap[0] = 0;

        let mut non_air = 0u32;
        for (mc_idx, &old_pal) in sec.voxels.iter().enumerate() {
            let old_idx = old_pal as usize;
            let mut new_pal = pal_remap[old_idx];
            if new_pal == u16::MAX {
                let st = sec.palette[old_idx].as_str();
                if st == "minecraft:air" || st.starts_with("%%FILTER_ME%%") {
                    new_pal = 0;
                } else {
                    new_pal = storage.palette.len() as u16;
                    storage.palette.push(st.to_string());
                }
                pal_remap[old_idx] = new_pal;
            }
            if new_pal != 0 {
                let lx = mc_idx & 15;
                let lz = (mc_idx >> 4) & 15;
                let ly = (mc_idx >> 8) & 15;
                storage.voxels[block_index(lx, ly, lz)] = new_pal;
                non_air += 1;
            }
        }
        storage.non_air_count = non_air;

        if non_air == 0 {
            continue;
        }

        storage.biomes = legacy_biomes
            .as_deref()
            .and_then(|b| ChunkParser::parse_legacy_section_biomes(b, sec_y));

        // Attach visual BlockEntities (banners, skulls) located inside this section.
        let sec_y_min = sec_idx << 12;
        let sec_y_max = sec_y_min + SECTION_VOLUME;
        for (&pos, be_comp) in &tile_entities {
            if pos >= sec_y_min && pos < sec_y_max {
                if let Some(mut entity_data) = decode_block_entity(be_comp, true) {
                    let local_mc = pos & 4095;
                    let lx = local_mc & 15;
                    let lz = (local_mc >> 4) & 15;
                    let ly = (local_mc >> 8) & 15;
                    let local_idx = block_index(lx, ly, lz) as u16;
                    let state_str = storage.get_local_state(lx, ly, lz);

                    match entity_data {
                        mtk_model::BlockEntityData::Skull {
                            ref mut kind,
                            ref profile,
                            ..
                        } if profile.is_none() => {
                            if let Some(resolved_kind) =
                                mtk_model::skull::SkullKind::from_block_name(state_str)
                            {
                                *kind = resolved_kind;
                            }
                        }
                        mtk_model::BlockEntityData::Banner {
                            ref mut base_color, ..
                        } => {
                            if let Some(resolved_color) =
                                mtk_model::banner::extract_banner_color_from_state(state_str)
                            {
                                *base_color = resolved_color.to_string();
                            }
                        }
                        _ => {}
                    }
                    storage.block_entities.insert(local_idx, entity_data);
                }
            }
        }

        out.push(storage);
    }

    Ok(out)
}

fn get_grid_state(grid: &[Option<LegacySection>; 16], pos: usize) -> &str {
    let sec_y = pos >> 12;
    if sec_y >= 16 {
        return "minecraft:air";
    }
    match &grid[sec_y] {
        Some(sec) => sec.get_state(pos & 4095),
        None => "minecraft:air",
    }
}

fn set_grid_state(grid: &mut [Option<LegacySection>; 16], pos: usize, state: &str) {
    let sec_y = pos >> 12;
    if sec_y < 16 {
        if let Some(ref mut sec) = grid[sec_y] {
            sec.set_state(pos & 4095, state);
        }
    }
}

fn apply_post_pass_fixups(
    grid: &mut [Option<LegacySection>; 16],
    tile_entities: &HashMap<usize, NbtCompound>,
    converted_from_alpha: bool,
) {
    let work: Vec<(usize, u8)> = grid
        .iter()
        .enumerate()
        .filter_map(|(sy, maybe_sec)| {
            let sec = maybe_sec.as_ref()?;
            let dy = sy << 12;
            Some(
                sec.fix_positions
                    .iter()
                    .map(move |&(id, idx)| (dy | idx as usize, id)),
            )
        })
        .flatten()
        .collect();

    for (pos, raw_id) in work {
        match raw_id {
            2 | 3 | 110 => {
                let target_name = match raw_id {
                    2 => "minecraft:grass_block",
                    3 => "minecraft:podzol",
                    _ => "minecraft:mycelium",
                };
                if split_state(get_grid_state(grid, pos)).0 == target_name && (pos >> 8) < 255 {
                    let above_name = split_state(get_grid_state(grid, pos + 256)).0;
                    if matches!(above_name, "minecraft:snow" | "minecraft:snow_block") {
                        set_grid_state(grid, pos, &format!("{target_name}[snowy=true]"));
                    }
                }
            }
            25 => {
                if let Some(te) = tile_entities.get(&pos) {
                    let powered = te.byte("powered").unwrap_or(0) != 0;
                    let note = te
                        .byte("note")
                        .map(|b| b as i32)
                        .or_else(|| te.int("note"))
                        .unwrap_or(0)
                        .clamp(0, 24);
                    set_grid_state(
                        grid,
                        pos,
                        &format!("minecraft:note_block[note={note},powered={powered}]"),
                    );
                }
            }
            26 => {
                if let Some(te) = tile_entities.get(&pos) {
                    let color = te
                        .int("color")
                        .or_else(|| te.byte("color").map(|b| b as i32))
                        .unwrap_or(14);
                    if (0..16).contains(&color) && color != 14 {
                        let cur = get_grid_state(grid, pos);
                        if let Some(rest) = cur.strip_prefix("minecraft:red_bed") {
                            let cname = dye_color_id_to_name(color);
                            set_grid_state(grid, pos, &format!("minecraft:{cname}_bed{rest}"));
                        }
                    }
                }
            }
            64 | 71 | 193..=197 => {
                let lower = get_grid_state(grid, pos).to_string();
                if get_prop(&lower, "half") == Some("lower") && (pos >> 8) < 255 {
                    let above_pos = pos + 256;
                    let upper = get_grid_state(grid, above_pos).to_string();
                    let name = split_state(&lower).0;
                    if name.ends_with("_door") && name == split_state(&upper).0 {
                        let facing = get_prop(&lower, "facing").unwrap_or("north");
                        let open = get_prop(&lower, "open").unwrap_or("false");
                        let hinge = if converted_from_alpha {
                            "left"
                        } else {
                            get_prop(&upper, "hinge").unwrap_or("left")
                        };
                        let powered = if converted_from_alpha {
                            "false"
                        } else {
                            get_prop(&upper, "powered").unwrap_or("false")
                        };
                        set_grid_state(
                            grid,
                            pos,
                            &format!(
                                "{name}[facing={facing},half=lower,hinge={hinge},open={open},powered={powered}]"
                            ),
                        );
                        set_grid_state(
                            grid,
                            above_pos,
                            &format!(
                                "{name}[facing={facing},half=upper,hinge={hinge},open={open},powered={powered}]"
                            ),
                        );
                    }
                }
            }
            86 => {
                if split_state(get_grid_state(grid, pos)).0 == "minecraft:carved_pumpkin"
                    && (pos >> 8) > 0
                {
                    let below_name = split_state(get_grid_state(grid, pos - 256)).0;
                    if matches!(below_name, "minecraft:grass_block" | "minecraft:dirt") {
                        set_grid_state(grid, pos, "minecraft:pumpkin");
                    }
                }
            }
            140 => {
                if let Some(te) = tile_entities.get(&pos) {
                    let item = te
                        .string("Item")
                        .map(|s| s.as_str().to_string())
                        .unwrap_or_default();
                    let data = te
                        .int("Data")
                        .or_else(|| te.byte("Data").map(|b| b as i32))
                        .unwrap_or(0);
                    let mapped = map_flower_pot(&item, data);
                    set_grid_state(grid, pos, mapped);
                }
            }
            144 => {
                let cur = get_grid_state(grid, pos).to_string();
                let facing = get_prop(&cur, "facing").unwrap_or("up");
                let (skull_type, rot) = match tile_entities.get(&pos) {
                    Some(te) => (
                        te.byte("SkullType")
                            .map(|b| b as i32)
                            .or_else(|| te.int("SkullType"))
                            .unwrap_or(0),
                        te.byte("Rot")
                            .map(|b| b as i32)
                            .or_else(|| te.int("Rot"))
                            .unwrap_or(0)
                            & 15,
                    ),
                    None => (0, 0),
                };
                let (species, suffix) = match skull_type {
                    1 => ("wither_skeleton", "skull"),
                    2 => ("zombie", "head"),
                    3 => ("player", "head"),
                    4 => ("creeper", "head"),
                    5 => ("dragon", "head"),
                    _ => ("skeleton", "skull"),
                };
                let new_state = if matches!(facing, "north" | "south" | "east" | "west") {
                    format!("minecraft:{species}_wall_{suffix}[facing={facing}]")
                } else {
                    format!("minecraft:{species}_{suffix}[rotation={rot}]")
                };
                set_grid_state(grid, pos, &new_state);
            }
            175 => {
                let cur = get_grid_state(grid, pos);
                if get_prop(cur, "half") == Some("upper") && (pos >> 8) > 0 {
                    let below_name = split_state(get_grid_state(grid, pos - 256)).0;
                    if matches!(
                        below_name,
                        "minecraft:sunflower"
                            | "minecraft:lilac"
                            | "minecraft:tall_grass"
                            | "minecraft:large_fern"
                            | "minecraft:rose_bush"
                            | "minecraft:peony"
                    ) {
                        set_grid_state(grid, pos, &format!("{below_name}[half=upper]"));
                    }
                }
            }
            176 | 177 => {
                if let Some(te) = tile_entities.get(&pos) {
                    let raw_base = te
                        .int("Base")
                        .or_else(|| te.byte("Base").map(|b| b as i32))
                        .unwrap_or(15);
                    let color_idx = 15 - raw_base;
                    if (0..16).contains(&color_idx) && color_idx != 0 {
                        let cur = get_grid_state(grid, pos);
                        let cname = dye_color_id_to_name(color_idx);
                        if let Some(rest) = cur.strip_prefix("minecraft:white_") {
                            set_grid_state(grid, pos, &format!("minecraft:{cname}_{rest}"));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // Keep `with_prop` used cleanly when properties need updating, or silence dead_code if unused.
    let _ = with_prop;
}

fn map_flower_pot(item: &str, data: i32) -> &'static str {
    let namespaced = if item.is_empty() || item.contains(':') {
        item
    } else {
        // Handle bare unnamespaced item names if encountered in older saves.
        match item {
            "red_flower" => "minecraft:red_flower",
            "yellow_flower" => "minecraft:yellow_flower",
            "sapling" => "minecraft:sapling",
            "red_mushroom" => "minecraft:red_mushroom",
            "brown_mushroom" => "minecraft:brown_mushroom",
            "deadbush" => "minecraft:deadbush",
            "tallgrass" => "minecraft:tallgrass",
            "cactus" => "minecraft:cactus",
            _ => item,
        }
    };
    match (namespaced, data) {
        ("minecraft:red_flower", 0) => "minecraft:potted_poppy",
        ("minecraft:red_flower", 1) => "minecraft:potted_blue_orchid",
        ("minecraft:red_flower", 2) => "minecraft:potted_allium",
        ("minecraft:red_flower", 3) => "minecraft:potted_azure_bluet",
        ("minecraft:red_flower", 4) => "minecraft:potted_red_tulip",
        ("minecraft:red_flower", 5) => "minecraft:potted_orange_tulip",
        ("minecraft:red_flower", 6) => "minecraft:potted_white_tulip",
        ("minecraft:red_flower", 7) => "minecraft:potted_pink_tulip",
        ("minecraft:red_flower", 8) => "minecraft:potted_oxeye_daisy",
        ("minecraft:yellow_flower", 0) => "minecraft:potted_dandelion",
        ("minecraft:sapling", 0) => "minecraft:potted_oak_sapling",
        ("minecraft:sapling", 1) => "minecraft:potted_spruce_sapling",
        ("minecraft:sapling", 2) => "minecraft:potted_birch_sapling",
        ("minecraft:sapling", 3) => "minecraft:potted_jungle_sapling",
        ("minecraft:sapling", 4) => "minecraft:potted_acacia_sapling",
        ("minecraft:sapling", 5) => "minecraft:potted_dark_oak_sapling",
        ("minecraft:red_mushroom", 0) => "minecraft:potted_red_mushroom",
        ("minecraft:brown_mushroom", 0) => "minecraft:potted_brown_mushroom",
        ("minecraft:deadbush", 0) => "minecraft:potted_dead_bush",
        ("minecraft:tallgrass", 2) => "minecraft:potted_fern",
        ("minecraft:cactus", 0) => "minecraft:potted_cactus",
        _ => "minecraft:flower_pot",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::*;

    fn set_nibble(buf: &mut [u8], idx: usize, val: u8) {
        let byte_idx = idx >> 1;
        if (idx & 1) == 0 {
            buf[byte_idx] = (buf[byte_idx] & 0xF0) | (val & 0x0F);
        } else {
            buf[byte_idx] = (buf[byte_idx] & 0x0F) | ((val & 0x0F) << 4);
        }
    }

    #[test]
    fn test_parse_1_12_2_chunk_with_blocks_data_biomes_and_tile_entities() {
        let mut blocks = vec![0u8; SECTION_VOLUME];
        let mut data = vec![0u8; 2048];
        let mut set_blk = |x: usize, y: usize, z: usize, id: u8, meta: u8| {
            let idx = (y << 8) | (z << 4) | x;
            blocks[idx] = id;
            set_nibble(&mut data, idx, meta);
        };

        // (0,0,0): stone (id=1, meta=0)
        set_blk(0, 0, 0, 1, 0);
        // (1,0,0): colored bed foot (id=26, meta=0 south) -> TileEntity color=11 (blue)
        set_blk(1, 0, 0, 26, 0);
        // (2,0,0): standing banner (id=176, meta=10) -> TileEntity Base=1 (15-1=14 red)
        set_blk(2, 0, 0, 176, 10);
        // (3,0,0): player skull (id=144, meta=1 up) -> TileEntity SkullType=3, Rot=8
        set_blk(3, 0, 0, 144, 1);
        // (4,0,0): dragon wall skull (id=144, meta=2 north) -> TileEntity SkullType=5
        set_blk(4, 0, 0, 144, 2);
        // (5,0,0): flower pot (id=140, meta=0) -> TileEntity red_flower/0 -> potted_poppy
        set_blk(5, 0, 0, 140, 0);
        // (6,0,0) & (6,1,0): oak door lower (meta=0 east,open=false) + upper (meta=9 hinge=right,powered=false)
        set_blk(6, 0, 0, 64, 0);
        set_blk(6, 1, 0, 64, 9);
        // (7,0,0) & (7,1,0): double plant tall_grass (meta=2) + upper (meta=8)
        set_blk(7, 0, 0, 175, 2);
        set_blk(7, 1, 0, 175, 8);
        // (8,0,0): powered repeater (id=94, meta=0 south delay=1)
        set_blk(8, 0, 0, 94, 0);
        // (9,0,0): standing sign (id=63, meta=4) -> upgraded to oak_sign
        set_blk(9, 0, 0, 63, 4);

        let mut sec_comp = NbtCompound::new();
        sec_comp
            .entries
            .push((NbtStr::from_borrowed("Y"), NbtTag::Byte(0)));
        sec_comp
            .entries
            .push((NbtStr::from_borrowed("Blocks"), NbtTag::ByteArray(&blocks)));
        sec_comp
            .entries
            .push((NbtStr::from_borrowed("Data"), NbtTag::ByteArray(&data)));

        let mut te_list = Vec::new();
        let make_pos_te = |id: &str, x: i32, y: i32, z: i32| {
            let mut c = NbtCompound::new();
            c.entries.push((
                NbtStr::from_borrowed("id"),
                NbtTag::String(NbtStr::from_owned(id.to_string())),
            ));
            c.entries.push((NbtStr::from_borrowed("x"), NbtTag::Int(x)));
            c.entries.push((NbtStr::from_borrowed("y"), NbtTag::Int(y)));
            c.entries.push((NbtStr::from_borrowed("z"), NbtTag::Int(z)));
            c
        };

        let mut bed_te = make_pos_te("minecraft:bed", 1, 0, 0);
        bed_te
            .entries
            .push((NbtStr::from_borrowed("color"), NbtTag::Int(11)));
        te_list.push(bed_te);

        let mut banner_te = make_pos_te("Banner", 2, 0, 0);
        banner_te
            .entries
            .push((NbtStr::from_borrowed("Base"), NbtTag::Int(1))); // 15-1 = 14 (red)
        let mut pat = NbtCompound::new();
        pat.entries.push((
            NbtStr::from_borrowed("Pattern"),
            NbtTag::String(NbtStr::from_borrowed("cre")),
        ));
        pat.entries
            .push((NbtStr::from_borrowed("Color"), NbtTag::Int(15))); // 15-15 = 0 (white)
        banner_te.entries.push((
            NbtStr::from_borrowed("Patterns"),
            NbtTag::List(NbtList::Compound(vec![pat])),
        ));
        te_list.push(banner_te);

        let mut p_skull = make_pos_te("minecraft:skull", 3, 0, 0);
        p_skull
            .entries
            .push((NbtStr::from_borrowed("SkullType"), NbtTag::Byte(3)));
        p_skull
            .entries
            .push((NbtStr::from_borrowed("Rot"), NbtTag::Byte(8)));
        p_skull.entries.push((
            NbtStr::from_borrowed("ExtraType"),
            NbtTag::String(NbtStr::from_borrowed("Notch")),
        ));
        te_list.push(p_skull);

        let mut d_skull = make_pos_te("minecraft:skull", 4, 0, 0);
        d_skull
            .entries
            .push((NbtStr::from_borrowed("SkullType"), NbtTag::Byte(5)));
        te_list.push(d_skull);

        let mut pot_te = make_pos_te("minecraft:flower_pot", 5, 0, 0);
        pot_te.entries.push((
            NbtStr::from_borrowed("Item"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:red_flower")),
        ));
        pot_te
            .entries
            .push((NbtStr::from_borrowed("Data"), NbtTag::Int(0)));
        te_list.push(pot_te);

        let mut biomes = vec![1u8; 256];
        biomes[(2 << 4) | 2] = 3; // quart (0, *, 0) samples (x=2, z=2): mountains -> windswept_hills

        let mut level = NbtCompound::new();
        level
            .entries
            .push((NbtStr::from_borrowed("xPos"), NbtTag::Int(0)));
        level
            .entries
            .push((NbtStr::from_borrowed("zPos"), NbtTag::Int(0)));
        level
            .entries
            .push((NbtStr::from_borrowed("Biomes"), NbtTag::ByteArray(&biomes)));
        level.entries.push((
            NbtStr::from_borrowed("Sections"),
            NbtTag::List(NbtList::Compound(vec![sec_comp])),
        ));
        level.entries.push((
            NbtStr::from_borrowed("TileEntities"),
            NbtTag::List(NbtList::Compound(te_list)),
        ));

        let sections =
            parse_pre_flattening_chunk(&level, IVec3::ZERO, 0, 15).expect("parse 1.12 chunk");
        assert_eq!(sections.len(), 1);
        let sec = &sections[0];
        assert!(
            !sec.palette.iter().any(|s| s.contains("FILTER_ME")),
            "palette must not retain %%FILTER_ME%% sentinel entries"
        );
        assert!(
            !sec.palette.iter().any(|s| s == "minecraft:potted_cactus"),
            "palette must compact away overwritten pre-fixup entries"
        );

        assert_eq!(sec.get_local_state(0, 0, 0), "minecraft:stone");
        assert_eq!(
            sec.get_local_state(1, 0, 0),
            "minecraft:blue_bed[facing=south,occupied=false,part=foot]"
        );
        assert_eq!(
            sec.get_local_state(2, 0, 0),
            "minecraft:red_banner[rotation=10]"
        );
        assert_eq!(
            sec.get_local_state(3, 0, 0),
            "minecraft:player_head[rotation=8]"
        );
        assert_eq!(
            sec.get_local_state(4, 0, 0),
            "minecraft:dragon_wall_head[facing=north]"
        );
        assert_eq!(sec.get_local_state(5, 0, 0), "minecraft:potted_poppy");
        assert_eq!(
            sec.get_local_state(6, 0, 0),
            "minecraft:oak_door[facing=east,half=lower,hinge=right,open=false,powered=false]"
        );
        assert_eq!(
            sec.get_local_state(6, 1, 0),
            "minecraft:oak_door[facing=east,half=upper,hinge=right,open=false,powered=false]"
        );
        assert_eq!(
            sec.get_local_state(7, 0, 0),
            "minecraft:tall_grass[half=lower]"
        );
        assert_eq!(
            sec.get_local_state(7, 1, 0),
            "minecraft:tall_grass[half=upper]"
        );
        assert_eq!(
            sec.get_local_state(8, 0, 0),
            "minecraft:repeater[delay=1,facing=south,locked=false,powered=true]"
        );
        assert_eq!(
            sec.get_local_state(9, 0, 0),
            "minecraft:oak_sign[rotation=4]"
        );

        let sec_biomes = sec.biomes.as_ref().expect("1.12 biomes must exist");
        assert_eq!(
            sec_biomes.get_quart_biome(0, 0, 0),
            Some("minecraft:windswept_hills")
        );

        match sec
            .get_block_entity(block_index(2, 0, 0) as u16)
            .expect("banner BE")
        {
            mtk_voxel::BlockEntityData::Banner {
                base_color,
                patterns,
            } => {
                assert_eq!(base_color, "red");
                assert_eq!(patterns[0].pattern, "cre");
                assert_eq!(patterns[0].color, "white");
            }
            _ => panic!("Expected Banner"),
        }
        match sec
            .get_block_entity(block_index(4, 0, 0) as u16)
            .expect("dragon skull BE")
        {
            mtk_voxel::BlockEntityData::Skull { kind, .. } => {
                assert_eq!(*kind, mtk_model::skull::SkullKind::Dragon);
            }
            _ => panic!("Expected Dragon Skull"),
        }
    }
}
