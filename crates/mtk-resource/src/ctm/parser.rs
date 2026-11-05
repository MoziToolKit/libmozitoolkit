use crate::ctm::types::{BlockMatch, ConnectLogic, CtmMethod, CtmRule, CtmSymmetry};
use crate::identifier::{ResourceLocation, DEFAULT_NAMESPACE};
use mtk_core::DirMask;
use std::collections::HashMap;

pub(crate) fn parse_ctm_properties(
    source_path: &str,
    namespace: &str,
    content: &str,
) -> Option<CtmRule> {
    let mut props: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("//") {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once('=') {
            props.insert(k.trim().to_lowercase(), v.trim().to_string());
        }
    }

    let parent_dir = if let Some(idx) = source_path.rfind('/') {
        &source_path[..idx + 1]
    } else {
        ""
    };

    // 1. Method
    let method_str = props.get("method").map(|s| s.as_str()).unwrap_or("ctm");
    let inner_seams = props
        .get("innerseams")
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let method = match method_str {
        "ctm" | "overlay_ctm" => CtmMethod::Full { inner_seams },
        "ctm_compact" => CtmMethod::Compact { inner_seams },
        "horizontal" => CtmMethod::Horizontal,
        "vertical" => CtmMethod::Vertical,
        "horizontal+vertical" => CtmMethod::HorizontalVertical,
        "vertical+horizontal" => CtmMethod::VerticalHorizontal,
        "top" => CtmMethod::Top,
        "repeat" | "overlay_repeat" => {
            let width = props.get("width").and_then(|w| w.parse().ok()).unwrap_or(2);
            let height = props
                .get("height")
                .and_then(|h| h.parse().ok())
                .unwrap_or(2);
            CtmMethod::Repeat { width, height }
        }
        "random" | "overlay_random" => {
            let symmetry = props
                .get("symmetry")
                .map(|s| CtmSymmetry::parse(s))
                .unwrap_or_default();
            let linked = props
                .get("linked")
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            let mut weights = Vec::new();
            let mut total_weight = 0.0f32;
            if let Some(w_str) = props.get("weights") {
                for token in w_str.split_whitespace() {
                    if let Ok(val) = token.parse::<f32>() {
                        weights.push(val);
                        total_weight += val;
                    }
                }
            }
            CtmMethod::Random {
                weights,
                total_weight,
                symmetry,
                linked,
            }
        }
        "fixed" | "overlay_fixed" => CtmMethod::Fixed,
        "overlay" => CtmMethod::Overlay,
        _ => CtmMethod::Full { inner_seams: false },
    };

    // 2. Tiles
    let mut tiles: Vec<Option<ResourceLocation>> = Vec::new();
    if let Some(tiles_str) = props.get("tiles") {
        for token in tiles_str.split_whitespace() {
            if token.eq_ignore_ascii_case("<skip>") || token.eq_ignore_ascii_case("<default>") {
                tiles.push(None);
            } else if token.contains('-') {
                // Range, e.g. 0-46
                if let Some((start_s, end_s)) = token.split_once('-') {
                    if let (Ok(start), Ok(end)) = (start_s.parse::<u32>(), end_s.parse::<u32>()) {
                        for i in start..=end {
                            let tile_path = format!("{}{}", parent_dir, i);
                            tiles.push(Some(ResourceLocation::new(namespace, tile_path)));
                        }
                    }
                }
            } else if token.contains(':') {
                // Explicit namespace in tile: "modid:path"
                if let Ok(loc) = ResourceLocation::parse_texture_path(token) {
                    tiles.push(Some(loc));
                } else if let Ok(loc) = ResourceLocation::parse(token) {
                    tiles.push(Some(loc));
                }
            } else if token.contains('/') {
                let clean = token.strip_prefix("assets/").unwrap_or(token);
                tiles.push(Some(
                    ResourceLocation::parse_texture_path(clean)
                        .unwrap_or_else(|_| ResourceLocation::new(namespace, clean)),
                ));
            } else {
                let clean_token = token.strip_suffix(".png").unwrap_or(token);
                let tile_path = format!("{}{}", parent_dir, clean_token);
                tiles.push(Some(ResourceLocation::new(namespace, tile_path)));
            }
        }
    }

    // 3. Match Blocks / Match Tiles (Preserving context namespace)
    let mut match_blocks = Vec::new();
    if let Some(mb) = props.get("matchblocks") {
        for token in mb.split_whitespace() {
            match_blocks.push(BlockMatch::parse_with_namespace(token, namespace));
        }
    }

    let mut match_tiles = Vec::new();
    if let Some(mt) = props.get("matchtiles") {
        for token in mt.split_whitespace() {
            let clean = token.strip_suffix(".png").unwrap_or(token);
            if clean.contains(':') {
                if let Ok(loc) = ResourceLocation::parse(clean) {
                    match_tiles.push(loc);
                }
            } else {
                match_tiles.push(ResourceLocation::new(namespace, format!("block/{}", clean)));
            }
        }
    }

    // Default match from filename if neither specified
    let file_stem = source_path
        .rsplit('/')
        .next()
        .unwrap_or(source_path)
        .strip_suffix(".properties")
        .unwrap_or(source_path);
    if match_blocks.is_empty() && match_tiles.is_empty() {
        if let Some(stripped) = file_stem.strip_prefix("block_") {
            match_blocks.push(BlockMatch::parse_with_namespace(stripped, namespace));
        } else {
            match_tiles.push(ResourceLocation::new(
                namespace,
                format!("block/{}", file_stem),
            ));
        }
    }

    // 4. Connect Logic
    let connect_logic = if let Some(ct) = props.get("connecttiles") {
        let list = ct
            .split_whitespace()
            .map(|s| {
                let clean = s.strip_suffix(".png").unwrap_or(s);
                if clean.contains(':') {
                    ResourceLocation::parse(clean)
                        .unwrap_or_else(|_| ResourceLocation::new(namespace, clean))
                } else {
                    ResourceLocation::new(namespace, format!("block/{}", clean))
                }
            })
            .collect();
        ConnectLogic::Textures(list)
    } else if let Some(cb) = props.get("connectblocks") {
        let list = cb
            .split_whitespace()
            .map(|s| {
                if s.contains(':') {
                    s.to_string()
                } else {
                    format!("{}:{}", namespace, s)
                }
            })
            .collect();
        ConnectLogic::BlockNames(list)
    } else if let Some(conn) = props.get("connect") {
        match conn.as_str() {
            "tile" => ConnectLogic::SameTile,
            "state" => ConnectLogic::SameState,
            _ => ConnectLogic::SameBlock,
        }
    } else if !match_tiles.is_empty() {
        ConnectLogic::SameTile
    } else {
        ConnectLogic::SameBlock
    };

    // 5. Faces
    let mut faces = DirMask::all();
    if let Some(faces_str) = props.get("faces") {
        let mut custom_faces = DirMask::empty();
        for f in faces_str.split_whitespace() {
            match f.to_lowercase().as_str() {
                "all" => custom_faces |= DirMask::all(),
                "sides" => {
                    custom_faces |= DirMask::NORTH | DirMask::SOUTH | DirMask::EAST | DirMask::WEST
                }
                "north" => custom_faces |= DirMask::NORTH,
                "south" => custom_faces |= DirMask::SOUTH,
                "east" => custom_faces |= DirMask::EAST,
                "west" => custom_faces |= DirMask::WEST,
                "up" | "top" => custom_faces |= DirMask::UP,
                "down" | "bottom" => custom_faces |= DirMask::DOWN,
                _ => {}
            }
        }
        if !custom_faces.is_empty() {
            faces = custom_faces;
        }
    }

    // 6. Weight / Priority
    let priority = props
        .get("weight")
        .and_then(|w| w.parse().ok())
        .unwrap_or(0);

    // 7. Biomes & Heights
    let biomes = props.get("biomes").map(|b| {
        b.split_whitespace()
            .map(|s| {
                if s.contains(':') {
                    s.to_string()
                } else {
                    format!("{}:{}", DEFAULT_NAMESPACE, s)
                }
            })
            .collect()
    });

    let height_ranges = props.get("heights").map(|h| {
        let mut ranges = Vec::new();
        for token in h.split_whitespace() {
            if let Some((min_s, max_s)) = token.split_once('-') {
                if let (Ok(min), Ok(max)) = (min_s.parse(), max_s.parse()) {
                    ranges.push((min, max));
                }
            } else if let Ok(val) = token.parse() {
                ranges.push((val, val));
            }
        }
        ranges
    });

    let tint_index = props.get("tintindex").and_then(|t| t.parse().ok());
    let tint_block = props.get("tintblock").cloned();

    Some(CtmRule {
        name: file_stem.to_string(),
        source_path: source_path.to_string(),
        priority,
        method,
        tiles,
        match_blocks,
        match_tiles,
        connect_logic,
        faces,
        biomes,
        height_ranges,
        tint_index,
        tint_block,
    })
}
