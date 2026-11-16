//! Universal NBT and Component BlockEntity decoder for mtk-save.
//!
//! Converts raw MCA `block_entities` compounds into high-level typed `BlockEntityData`,
//! supporting both pre-1.20.5 Tag structures and 1.20.5+ Data Component structures.

use crate::nbt::NbtCompound;
use mtk_model::banner::BannerPatternLayer;
use mtk_model::skull::{PlayerProfileData, SkullKind};
use mtk_model::{BlockEntityData, ItemComponentsData};

/// Decodes an NBT Compound from a chunk's `block_entities` list into a typed `BlockEntityData`.
///
/// Returns `None` for unsupported or non-visual block entities (e.g. standard storage chests, hoppers).
pub(crate) fn decode_block_entity(comp: &NbtCompound) -> Option<BlockEntityData> {
    let id_str = comp
        .string("id")
        .or_else(|| comp.string("Id"))?
        .as_str()
        .to_string();
    let clean_id = id_str.strip_prefix("minecraft:").unwrap_or(&id_str);

    match clean_id {
        "decorated_pot" => decode_decorated_pot(comp),
        "banner" => decode_banner(comp),
        "skull" => decode_skull(comp),
        _ => None,
    }
}

/// Decodes Decorated Pot NBT data (pre-1.20.5 `sherds` list and 1.20.5+ `minecraft:pot_decorations` component).
fn decode_decorated_pot(comp: &NbtCompound) -> Option<BlockEntityData> {
    let mut sherds = [None, None, None, None];

    // 1. Check pre-1.20.5 `sherds` list
    if let Some(sherd_list) = comp
        .list("sherds")
        .or_else(|| comp.list("Sherds"))
        .and_then(|l| l.strings())
    {
        for (i, s) in sherd_list.iter().enumerate().take(4) {
            let s_ref = s.as_str();
            if s_ref != "minecraft:brick" && s_ref != "brick" && !s_ref.is_empty() {
                sherds[i] = Some(s_ref.to_string());
            }
        }
    }
    // 2. Check 1.20.5+ component `minecraft:pot_decorations` (direct or nested)
    else {
        let dec_comp = comp
            .compound("minecraft:pot_decorations")
            .or_else(|| comp.compound("pot_decorations"))
            .or_else(|| {
                comp.compound("components").and_then(|c| {
                    c.compound("minecraft:pot_decorations")
                        .or_else(|| c.compound("pot_decorations"))
                })
            });

        let dec_list = comp
            .list("minecraft:pot_decorations")
            .or_else(|| comp.list("pot_decorations"))
            .or_else(|| {
                comp.compound("components").and_then(|c| {
                    c.list("minecraft:pot_decorations")
                        .or_else(|| c.list("pot_decorations"))
                })
            });

        if let Some(decorations) = dec_comp {
            let slots = ["back", "left", "right", "front"];
            for (i, slot) in slots.iter().enumerate() {
                if let Some(s) = decorations.string(slot) {
                    let s_ref = s.as_str();
                    if s_ref != "minecraft:brick" && s_ref != "brick" && !s_ref.is_empty() {
                        sherds[i] = Some(s_ref.to_string());
                    }
                }
            }
        } else if let Some(l) = dec_list.and_then(|l| l.strings()) {
            for (i, s) in l.iter().enumerate().take(4) {
                let s_ref = s.as_str();
                if s_ref != "minecraft:brick" && s_ref != "brick" && !s_ref.is_empty() {
                    sherds[i] = Some(s_ref.to_string());
                }
            }
        }
    }

    Some(mtk_model::BlockEntityData::DecoratedPot { sherds })
}

/// Decodes Banner NBT data (pre-1.20.5 `Patterns` list and 1.20.5+ `minecraft:banner_patterns` component).
fn decode_banner(comp: &NbtCompound) -> Option<BlockEntityData> {
    // 1. Determine base color
    let base_color = comp
        .string("BaseColor")
        .or_else(|| comp.string("base_color"))
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|| "white".to_string());

    let mut patterns = Vec::new();

    // 2. Pre-1.20.5 `Patterns` list
    if let Some(pattern_list) = comp
        .list("Patterns")
        .or_else(|| comp.list("patterns"))
        .and_then(|l| l.compounds())
    {
        for p_comp in pattern_list {
            let pat = p_comp
                .string("Pattern")
                .or_else(|| p_comp.string("pattern"))
                .map(|s| s.as_str().to_string())
                .unwrap_or_else(|| "base".to_string());

            let col = p_comp
                .string("Color")
                .or_else(|| p_comp.string("color"))
                .map(|s| s.as_str().to_string())
                .unwrap_or_else(|| "white".to_string());

            patterns.push(BannerPatternLayer::new(pat, col));
        }
    }
    // 3. 1.20.5+ component `minecraft:banner_patterns` (direct or nested)
    else {
        let pattern_list = comp
            .list("minecraft:banner_patterns")
            .or_else(|| comp.list("banner_patterns"))
            .or_else(|| {
                comp.compound("components").and_then(|c| {
                    c.list("minecraft:banner_patterns")
                        .or_else(|| c.list("banner_patterns"))
                })
            });

        if let Some(comp_patterns) = pattern_list.and_then(|l| l.compounds()) {
            for p_comp in comp_patterns {
                let pat = p_comp
                    .string("pattern")
                    .or_else(|| p_comp.string("pattern_id"))
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| "base".to_string());

                let col = p_comp
                    .string("color")
                    .map(|s| s.as_str().to_string())
                    .unwrap_or_else(|| "white".to_string());

                patterns.push(BannerPatternLayer::new(pat, col));
            }
        }
    }

    Some(mtk_model::BlockEntityData::Banner {
        base_color,
        patterns,
    })
}

/// Decodes a string or int_array UUID from an NBT compound (MC 1.16+ uses 4-int array).
fn decode_nbt_uuid(comp: &NbtCompound, key: &str) -> Option<String> {
    if let Some(s) = comp.string(key) {
        return Some(s.as_str().to_string());
    }
    if let Some(arr) = comp.int_array(key) {
        if arr.len() == 4 {
            let u = ((arr[0] as u32 as u128) << 96)
                | ((arr[1] as u32 as u128) << 64)
                | ((arr[2] as u32 as u128) << 32)
                | (arr[3] as u32 as u128);
            return Some(format!("{:032x}", u));
        }
    }
    None
}

/// Decodes Skull / Head NBT data (Rot, SkullOwner, and 1.20.5+ `profile` / `minecraft:profile` component).
fn decode_skull(comp: &NbtCompound) -> Option<BlockEntityData> {
    let rotation = comp
        .byte("Rot")
        .or_else(|| comp.int("Rot").map(|i| i as i8))
        .map(|r| (r as u8) & 0x0F);

    let profile = decode_profile(comp);
    let kind = if profile.is_some() {
        SkullKind::Player
    } else {
        SkullKind::Skeleton
    };

    Some(mtk_model::BlockEntityData::Skull {
        kind,
        rotation,
        profile,
    })
}

/// Decodes player profile data from either 1.20.5+ `profile`, pre-1.20.5 `SkullOwner`,
/// or component-based `minecraft:profile`.
fn decode_profile(comp: &NbtCompound) -> Option<PlayerProfileData> {
    // 1. Direct profile compound (1.20.5+ block entity), SkullOwner (pre-1.20.5),
    // or component `minecraft:profile`
    let prof_comp = comp
        .compound("profile")
        .or_else(|| comp.compound("SkullOwner"))
        .or_else(|| comp.compound("minecraft:profile"))
        .or_else(|| {
            comp.compound("components").and_then(|c| {
                c.compound("minecraft:profile")
                    .or_else(|| c.compound("profile"))
            })
        });

    if let Some(p) = prof_comp {
        let name = p
            .string("name")
            .or_else(|| p.string("Name"))
            .map(|s| s.as_str().to_string());
        let id = decode_nbt_uuid(p, "id").or_else(|| decode_nbt_uuid(p, "Id"));

        let mut skin_b64: Option<String> = None;
        // In 1.20.5+, properties is a list of compounds [{name: "textures", value: "..."}]
        if let Some(props) = p
            .list("properties")
            .or_else(|| p.list("Properties"))
            .and_then(|l| l.compounds())
        {
            for prop in &props {
                if prop
                    .string("name")
                    .or_else(|| prop.string("Name"))
                    .map(|s| s.as_str().eq_ignore_ascii_case("textures"))
                    .unwrap_or(false)
                {
                    skin_b64 = prop
                        .string("value")
                        .or_else(|| prop.string("Value"))
                        .map(|s| s.as_str().to_string());
                    break;
                }
            }
        }
        // In pre-1.20.5, Properties is a compound containing textures list: Properties: { textures: [{ Value: "..." }] }
        if skin_b64.is_none() {
            if let Some(props_comp) = p
                .compound("Properties")
                .or_else(|| p.compound("properties"))
            {
                if let Some(tex_list) = props_comp
                    .list("textures")
                    .or_else(|| props_comp.list("Textures"))
                    .and_then(|l| l.compounds())
                {
                    if let Some(first) = tex_list.first() {
                        skin_b64 = first
                            .string("Value")
                            .or_else(|| first.string("value"))
                            .map(|s| s.as_str().to_string());
                    }
                }
            }
        }

        return Some(PlayerProfileData::new(name, id, None, skin_b64));
    }

    // 2. Direct string owner / profile
    if let Some(s) = comp.string("profile").or_else(|| comp.string("SkullOwner")) {
        return Some(PlayerProfileData::new(
            Some(s.as_str().to_string()),
            None,
            None,
            None,
        ));
    }

    // 3. Direct int array UUID owner / profile
    let arr = comp
        .int_array("profile")
        .or_else(|| comp.int_array("SkullOwner"));
    if let Some(arr) = arr {
        if arr.len() == 4 {
            let u = ((arr[0] as u32 as u128) << 96)
                | ((arr[1] as u32 as u128) << 64)
                | ((arr[2] as u32 as u128) << 32)
                | (arr[3] as u32 as u128);
            return Some(PlayerProfileData::new(
                None,
                Some(format!("{:032x}", u)),
                None,
                None,
            ));
        }
    }

    None
}

/// Decodes high-order item components and NBT payloads from an Item compound
/// (supporting both pre-1.20.5 `tag` structures and 1.20.5+ `components` structures).
pub(crate) fn decode_item_components(
    item_id: &str,
    item_comp: &NbtCompound,
) -> Option<ItemComponentsData> {
    let clean_id = item_id.strip_prefix("minecraft:").unwrap_or(item_id);

    // 1. Resolve BlockEntityData if applicable
    let mut block_entity: Option<BlockEntityData> = None;

    // Check modern components (1.20.5+)
    if let Some(comp_map) = item_comp.compound("components") {
        if comp_map.compound("minecraft:pot_decorations").is_some()
            || comp_map.list("minecraft:pot_decorations").is_some()
            || comp_map.compound("pot_decorations").is_some()
            || comp_map.list("pot_decorations").is_some()
        {
            block_entity = decode_decorated_pot(comp_map);
        } else if comp_map.list("minecraft:banner_patterns").is_some()
            || comp_map.list("banner_patterns").is_some()
        {
            let mut be = decode_banner(comp_map);
            if let Some(BlockEntityData::Banner {
                ref mut base_color, ..
            }) = be
            {
                if base_color == "white" && clean_id.contains('_') {
                    if let Some((color_prefix, _)) = clean_id.split_once('_') {
                        *base_color = color_prefix.to_string();
                    }
                }
            }
            block_entity = be;
        } else if comp_map.compound("minecraft:profile").is_some()
            || comp_map.compound("profile").is_some()
        {
            block_entity = decode_skull(comp_map);
        } else if let Some(be_data) = comp_map
            .compound("minecraft:block_entity_data")
            .or_else(|| comp_map.compound("block_entity_data"))
        {
            block_entity = decode_block_entity(be_data);
            if block_entity.is_none() {
                if clean_id.contains("pot") {
                    block_entity = decode_decorated_pot(be_data);
                } else if clean_id.contains("banner") || clean_id.contains("shield") {
                    let mut be = decode_banner(be_data);
                    if let Some(BlockEntityData::Banner {
                        ref mut base_color, ..
                    }) = be
                    {
                        if base_color == "white" && clean_id.contains('_') {
                            if let Some((color_prefix, _)) = clean_id.split_once('_') {
                                *base_color = color_prefix.to_string();
                            }
                        }
                    }
                    block_entity = be;
                } else if clean_id.contains("head") || clean_id.contains("skull") {
                    block_entity = decode_skull(be_data);
                }
            }
        }
    }

    // Check legacy tag (pre-1.20.5)
    if block_entity.is_none() {
        if let Some(tag_map) = item_comp.compound("tag") {
            if let Some(be_tag) = tag_map.compound("BlockEntityTag") {
                block_entity = decode_block_entity(be_tag);
                if block_entity.is_none() {
                    if clean_id.contains("pot") {
                        block_entity = decode_decorated_pot(be_tag);
                    } else if clean_id.contains("banner") || clean_id.contains("shield") {
                        let mut be = decode_banner(be_tag);
                        if let Some(BlockEntityData::Banner {
                            ref mut base_color, ..
                        }) = be
                        {
                            if base_color == "white" && clean_id.contains('_') {
                                if let Some((color_prefix, _)) = clean_id.split_once('_') {
                                    *base_color = color_prefix.to_string();
                                }
                            }
                        }
                        block_entity = be;
                    } else if clean_id.contains("head") || clean_id.contains("skull") {
                        block_entity = decode_skull(be_tag);
                    }
                }
            } else if tag_map.compound("SkullOwner").is_some()
                || tag_map.string("SkullOwner").is_some()
            {
                block_entity = decode_skull(tag_map);
            }
        }
    }

    // 2. Resolve dyed_color
    let dyed_color = item_comp
        .compound("components")
        .and_then(|c| {
            c.int("minecraft:dyed_color")
                .or_else(|| c.int("dyed_color"))
                .or_else(|| {
                    c.compound("minecraft:dyed_color")
                        .or_else(|| c.compound("dyed_color"))
                        .and_then(|dc| dc.int("rgb").or_else(|| dc.int("color")))
                })
        })
        .or_else(|| {
            item_comp
                .compound("tag")
                .and_then(|t| t.compound("display"))
                .and_then(|d| d.int("color"))
        })
        .map(|c| c as u32);

    // 3. Resolve custom_name
    let custom_name = item_comp
        .compound("components")
        .and_then(|c| {
            c.string("minecraft:custom_name")
                .or_else(|| c.string("custom_name"))
        })
        .or_else(|| {
            item_comp
                .compound("tag")
                .and_then(|t| t.compound("display"))
                .and_then(|d| d.string("Name"))
        })
        .map(|s| s.as_str().to_string());

    // 4. Resolve has_glint
    let has_glint = item_comp
        .compound("components")
        .map(|c| {
            c.compound("minecraft:enchantments").is_some()
                || c.compound("enchantments").is_some()
                || c.byte("minecraft:enchantment_glint_override") == Some(1)
                || c.byte("enchantment_glint_override") == Some(1)
        })
        .unwrap_or(false)
        || item_comp
            .compound("tag")
            .map(|t| {
                t.list("Enchantments").is_some()
                    || t.list("enchantments").is_some()
                    || t.list("StoredEnchantments").is_some()
                    || t.list("ench").is_some()
            })
            .unwrap_or(false);

    if block_entity.is_none() && dyed_color.is_none() && custom_name.is_none() && !has_glint {
        return None;
    }

    Some(ItemComponentsData {
        block_entity,
        dyed_color,
        custom_name,
        has_glint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nbt::{NbtList, NbtStr, NbtTag};

    #[test]
    fn test_decode_banner_legacy_and_components() {
        // 1. Legacy Patterns test
        let mut banner_comp = NbtCompound::new();
        banner_comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:banner")),
        ));
        banner_comp.entries.push((
            NbtStr::from_borrowed("BaseColor"),
            NbtTag::String(NbtStr::from_borrowed("blue")),
        ));

        let mut p1 = NbtCompound::new();
        p1.entries.push((
            NbtStr::from_borrowed("Pattern"),
            NbtTag::String(NbtStr::from_borrowed("ts")),
        ));
        p1.entries.push((
            NbtStr::from_borrowed("Color"),
            NbtTag::String(NbtStr::from_borrowed("orange")),
        ));

        banner_comp.entries.push((
            NbtStr::from_borrowed("Patterns"),
            NbtTag::List(NbtList::Compound(vec![p1])),
        ));

        let decoded = decode_block_entity(&banner_comp).expect("Must decode banner");
        if let BlockEntityData::Banner {
            base_color,
            patterns,
        } = decoded
        {
            assert_eq!(base_color, "blue");
            assert_eq!(patterns.len(), 1);
            assert_eq!(patterns[0].pattern, "ts");
            assert_eq!(patterns[0].color, "orange");
        } else {
            panic!("Expected Banner variant");
        }
    }

    #[test]
    fn test_decode_player_head_with_profile() {
        let mut skull_comp = NbtCompound::new();
        skull_comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:skull")),
        ));
        skull_comp
            .entries
            .push((NbtStr::from_borrowed("SkullType"), NbtTag::Byte(3)));
        skull_comp
            .entries
            .push((NbtStr::from_borrowed("Rot"), NbtTag::Byte(8)));

        let mut owner_comp = NbtCompound::new();
        owner_comp.entries.push((
            NbtStr::from_borrowed("Name"),
            NbtTag::String(NbtStr::from_borrowed("Alex")),
        ));

        let mut props_comp = NbtCompound::new();
        let mut tex_entry = NbtCompound::new();
        // Base64 encoding of {"textures":{"SKIN":{"url":"http://textures.minecraft.net/texture/12345"}}}
        let b64 = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvMTIzNDUifX19";
        tex_entry.entries.push((
            NbtStr::from_borrowed("Value"),
            NbtTag::String(NbtStr::from_borrowed(b64)),
        ));
        props_comp.entries.push((
            NbtStr::from_borrowed("textures"),
            NbtTag::List(NbtList::Compound(vec![tex_entry])),
        ));

        owner_comp.entries.push((
            NbtStr::from_borrowed("Properties"),
            NbtTag::Compound(props_comp),
        ));
        skull_comp.entries.push((
            NbtStr::from_borrowed("SkullOwner"),
            NbtTag::Compound(owner_comp),
        ));

        let decoded = decode_block_entity(&skull_comp).expect("Must decode skull");
        if let BlockEntityData::Skull {
            kind,
            rotation,
            profile,
        } = decoded
        {
            assert_eq!(kind, SkullKind::Player);
            assert_eq!(rotation, Some(8));
            let prof = profile.expect("Profile must be present");
            assert_eq!(prof.name.as_deref(), Some("Alex"));
            assert_eq!(
                prof.skin_url.as_deref(),
                Some("http://textures.minecraft.net/texture/12345")
            );
            assert_eq!(prof.skin_hash(), Some("12345"));
        } else {
            panic!("Expected Skull variant");
        }
    }

    #[test]
    fn test_decode_player_head_with_modern_profile_component() {
        let mut skull_comp = NbtCompound::new();
        skull_comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:skull")),
        ));

        let mut prof_comp = NbtCompound::new();
        prof_comp.entries.push((
            NbtStr::from_borrowed("name"),
            NbtTag::String(NbtStr::from_borrowed("jaxlocke")),
        ));
        let mut id_bytes = Vec::new();
        for val in [1132644265i32, 301416900, -2114120302, -1759600980] {
            id_bytes.extend_from_slice(&val.to_be_bytes());
        }
        prof_comp
            .entries
            .push((NbtStr::from_borrowed("id"), NbtTag::IntArray(&id_bytes)));

        let mut tex_entry = NbtCompound::new();
        let b64 = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHA6Ly90ZXh0dXJlcy5taW5lY3JhZnQubmV0L3RleHR1cmUvYmFlNDY0YTBlZTMwNTRmMTI3Mzg0NWI0NDNlYTkyODdlOWExYjg2NWNlZTU1N2U3ZWMwYmJiMjhhY2U2ODZkIn19fQ==";
        tex_entry.entries.push((
            NbtStr::from_borrowed("name"),
            NbtTag::String(NbtStr::from_borrowed("textures")),
        ));
        tex_entry.entries.push((
            NbtStr::from_borrowed("value"),
            NbtTag::String(NbtStr::from_borrowed(b64)),
        ));

        prof_comp.entries.push((
            NbtStr::from_borrowed("properties"),
            NbtTag::List(NbtList::Compound(vec![tex_entry])),
        ));

        skull_comp.entries.push((
            NbtStr::from_borrowed("profile"),
            NbtTag::Compound(prof_comp),
        ));

        let decoded = decode_block_entity(&skull_comp).expect("Must decode modern 1.20.5+ skull");
        if let BlockEntityData::Skull { kind, profile, .. } = decoded {
            assert_eq!(kind, SkullKind::Player);
            let prof = profile.expect("Profile must be present");
            assert_eq!(prof.name.as_deref(), Some("jaxlocke"));
            assert_eq!(
                prof.skin_hash(),
                Some("bae464a0ee3054f1273845b443ea9287e9a1b865cee557e7ec0bbb28ace686d")
            );
        } else {
            panic!("Expected Skull variant");
        }
    }

    #[test]
    fn test_decode_item_components_legacy_pot_with_sherds() {
        let mut item_comp = NbtCompound::new();
        item_comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:decorated_pot")),
        ));

        let mut be_tag = NbtCompound::new();
        be_tag.entries.push((
            NbtStr::from_borrowed("sherds"),
            NbtTag::List(NbtList::String(vec![
                NbtStr::from_borrowed("minecraft:archer_pottery_sherd"),
                NbtStr::from_borrowed("minecraft:brick"),
                NbtStr::from_borrowed("minecraft:miner_pottery_sherd"),
                NbtStr::from_borrowed("minecraft:brick"),
            ])),
        ));

        let mut tag_comp = NbtCompound::new();
        tag_comp.entries.push((
            NbtStr::from_borrowed("BlockEntityTag"),
            NbtTag::Compound(be_tag),
        ));

        item_comp
            .entries
            .push((NbtStr::from_borrowed("tag"), NbtTag::Compound(tag_comp)));

        let decoded = decode_item_components("minecraft:decorated_pot", &item_comp)
            .expect("Must decode legacy pot item components");

        let be = decoded
            .block_entity
            .expect("Block entity data must be present");
        if let BlockEntityData::DecoratedPot { sherds } = be {
            assert_eq!(sherds[0].as_deref(), Some("minecraft:archer_pottery_sherd"));
            assert_eq!(sherds[1], None); // brick -> None
            assert_eq!(sherds[2].as_deref(), Some("minecraft:miner_pottery_sherd"));
            assert_eq!(sherds[3], None);
        } else {
            panic!("Expected DecoratedPot variant");
        }
    }

    #[test]
    fn test_decode_item_components_modern_pot_with_sherds() {
        let mut item_comp = NbtCompound::new();
        item_comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:decorated_pot")),
        ));

        let mut comp_map = NbtCompound::new();
        comp_map.entries.push((
            NbtStr::from_borrowed("minecraft:pot_decorations"),
            NbtTag::List(NbtList::String(vec![
                NbtStr::from_borrowed("minecraft:angler_pottery_sherd"),
                NbtStr::from_borrowed("minecraft:blade_pottery_sherd"),
                NbtStr::from_borrowed("minecraft:brewer_pottery_sherd"),
                NbtStr::from_borrowed("minecraft:burn_pottery_sherd"),
            ])),
        ));

        item_comp.entries.push((
            NbtStr::from_borrowed("components"),
            NbtTag::Compound(comp_map),
        ));

        let decoded = decode_item_components("minecraft:decorated_pot", &item_comp)
            .expect("Must decode modern pot item components");

        let be = decoded
            .block_entity
            .expect("Block entity data must be present");
        if let BlockEntityData::DecoratedPot { sherds } = be {
            assert_eq!(sherds[0].as_deref(), Some("minecraft:angler_pottery_sherd"));
            assert_eq!(sherds[1].as_deref(), Some("minecraft:blade_pottery_sherd"));
            assert_eq!(sherds[2].as_deref(), Some("minecraft:brewer_pottery_sherd"));
            assert_eq!(sherds[3].as_deref(), Some("minecraft:burn_pottery_sherd"));
        } else {
            panic!("Expected DecoratedPot variant");
        }
    }

    #[test]
    fn test_decode_item_components_dyed_leather_and_name_and_glint() {
        let mut item_comp = NbtCompound::new();
        item_comp.entries.push((
            NbtStr::from_borrowed("id"),
            NbtTag::String(NbtStr::from_borrowed("minecraft:leather_chestplate")),
        ));

        let mut comp_map = NbtCompound::new();
        comp_map.entries.push((
            NbtStr::from_borrowed("minecraft:dyed_color"),
            NbtTag::Int(0xFF0000), // Red
        ));
        comp_map.entries.push((
            NbtStr::from_borrowed("minecraft:custom_name"),
            NbtTag::String(NbtStr::from_borrowed("Hero Armor")),
        ));
        comp_map.entries.push((
            NbtStr::from_borrowed("minecraft:enchantment_glint_override"),
            NbtTag::Byte(1),
        ));

        item_comp.entries.push((
            NbtStr::from_borrowed("components"),
            NbtTag::Compound(comp_map),
        ));

        let decoded = decode_item_components("minecraft:leather_chestplate", &item_comp)
            .expect("Must decode dyed leather item components");

        assert_eq!(decoded.dyed_color, Some(0xFF0000));
        assert_eq!(decoded.custom_name.as_deref(), Some("Hero Armor"));
        assert!(decoded.has_glint);
    }
}
