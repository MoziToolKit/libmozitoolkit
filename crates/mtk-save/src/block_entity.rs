//! Universal NBT and Component BlockEntity decoder for mtk-save.
//!
//! Converts raw MCA `block_entities` compounds into high-level typed `BlockEntityData`,
//! supporting both pre-1.20.5 Tag structures and 1.20.5+ Data Component structures.

use crate::nbt::NbtCompound;
use mtk_model::banner::BannerPatternLayer;
use mtk_model::skull::{PlayerProfileData, SkullKind};
use mtk_model::BlockEntityData;

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
    // 2. Check 1.20.5+ component `minecraft:pot_decorations`
    else if let Some(components) = comp.compound("components") {
        if let Some(decorations) = components.compound("minecraft:pot_decorations") {
            let slots = ["back", "left", "right", "front"];
            for (i, slot) in slots.iter().enumerate() {
                if let Some(s) = decorations.string(slot) {
                    let s_ref = s.as_str();
                    if s_ref != "minecraft:brick" && s_ref != "brick" && !s_ref.is_empty() {
                        sherds[i] = Some(s_ref.to_string());
                    }
                }
            }
        } else if let Some(dec_list) = components
            .list("minecraft:pot_decorations")
            .and_then(|l| l.strings())
        {
            for (i, s) in dec_list.iter().enumerate().take(4) {
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
    // 3. 1.20.5+ component `minecraft:banner_patterns`
    else if let Some(components) = comp.compound("components") {
        if let Some(comp_patterns) = components
            .list("minecraft:banner_patterns")
            .and_then(|l| l.compounds())
        {
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
}
