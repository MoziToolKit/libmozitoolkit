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

    // 1. Method — Continuity's registry (`ContinuityClient`). An unknown method drops the rule;
    // silently falling back to 47-tile CTM applies the wrong algorithm.
    let method_str = props.get("method").map(|s| s.as_str()).unwrap_or("ctm");
    let inner_seams = props
        .get("innerseams")
        .map(|s| s.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    let method = match method_str {
        "ctm" => CtmMethod::Full,
        "ctm_compact" => CtmMethod::Compact,
        "horizontal" | "bookshelf" => CtmMethod::Horizontal,
        "vertical" => CtmMethod::Vertical,
        "horizontal+vertical" | "h+v" => CtmMethod::HorizontalVertical,
        "vertical+horizontal" | "v+h" => CtmMethod::VerticalHorizontal,
        "top" => CtmMethod::Top,
        "repeat" => {
            let width = props.get("width").and_then(|w| w.parse().ok()).unwrap_or(2);
            let height = props
                .get("height")
                .and_then(|h| h.parse().ok())
                .unwrap_or(2);
            CtmMethod::Repeat { width, height }
        }
        "random" => {
            let symmetry = props
                .get("symmetry")
                .map(|s| CtmSymmetry::parse(s))
                .unwrap_or_default();
            let linked = props
                .get("linked")
                .map(|s| s.eq_ignore_ascii_case("true"))
                .unwrap_or(false);
            // Continuity parses integer weights separated by `[ ,]` and expands `a-b` ranges;
            // non-positive values are rejected.
            let mut weights = Vec::new();
            let mut total_weight = 0.0f32;
            if let Some(w_str) = props.get("weights") {
                for token in w_str.split([' ', ',']).filter(|t| !t.is_empty()) {
                    let mut push = |weight: u32| {
                        weights.push(weight as f32);
                        total_weight += weight as f32;
                    };
                    if let Some((min_s, max_s)) = token.split_once('-') {
                        if let (Ok(min), Ok(max)) = (min_s.parse::<u32>(), max_s.parse::<u32>()) {
                            for weight in min..=max {
                                if weight > 0 {
                                    push(weight);
                                }
                            }
                            continue;
                        }
                    }
                    if let Ok(weight) = token.parse::<u32>() {
                        if weight > 0 {
                            push(weight);
                        }
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
        "fixed" => CtmMethod::Fixed,
        // Overlay methods need an extra blended layer, which this decoder does not emit yet
        // (backlog C-11). Treating them as their base method silently *replaces* the texture.
        _ => return None,
    };

    // Continuity's `TileAmountValidator`: a 47-tile method with four tiles is a broken rule.
    let requires: Option<(u32, bool)> = match &method {
        CtmMethod::Full => Some((47, true)),
        CtmMethod::Compact => Some((5, true)),
        CtmMethod::Horizontal | CtmMethod::Vertical => Some((4, false)),
        CtmMethod::HorizontalVertical | CtmMethod::VerticalHorizontal => Some((7, false)),
        CtmMethod::Top | CtmMethod::Fixed => Some((1, false)),
        CtmMethod::Overlay => Some((17, true)),
        CtmMethod::Random { .. } | CtmMethod::Repeat { .. } => None,
    };

    // 2. Tiles — Continuity requires the property, splits on `[ ,]`, understands `<skip>`,
    // `<default>`, `a-b` ranges and `namespace:path` / relative / absolute spellings.
    let mut tiles: Vec<Option<ResourceLocation>> = Vec::new();
    {
        let tiles_str = props.get("tiles")?;
        for token in tiles_str
            .split([' ', ','])
            .filter(|token| !token.is_empty())
        {
            if token.ends_with("<skip>") || token.ends_with("<skip>.png") {
                tiles.push(None);
                continue;
            }
            if token.ends_with("<default>") || token.ends_with("<default>.png") {
                tiles.push(None);
                continue;
            }

            let mut handled = false;
            if let Some((start_s, end_s)) = token.split_once('-') {
                if let (Ok(start), Ok(end)) = (start_s.parse::<u32>(), end_s.parse::<u32>()) {
                    for index in start..=end {
                        tiles.push(Some(ResourceLocation::new(
                            namespace,
                            format!("{parent_dir}{index}"),
                        )));
                    }
                    handled = true;
                }
            }
            if handled {
                continue;
            }

            if token.contains(':') {
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
                tiles.push(Some(ResourceLocation::new(
                    namespace,
                    format!("{parent_dir}{clean_token}"),
                )));
            }
        }
    }

    // Continuity's `TileAmountValidator`: reject rules whose tile count the method cannot use.
    if let Some((amount, at_least)) = requires {
        let count = tiles.len() as u32;
        let ok = if at_least {
            count >= amount
        } else {
            count == amount
        };
        if !ok {
            return None;
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
        match conn.trim().to_lowercase().as_str() {
            "tile" => ConnectLogic::SameTile,
            "state" => ConnectLogic::SameState,
            _ => ConnectLogic::SameBlock,
        }
    } else if !match_tiles.is_empty() {
        ConnectLogic::SameTile
    } else {
        ConnectLogic::SameBlock
    };

    // 5. Faces — Continuity: unknown tokens are skipped, an empty result invalidates the rule,
    // and `all` covers every face even when mixed with other tokens.
    let mut faces = DirMask::all();
    if let Some(faces_str) = props.get("faces") {
        let mut custom_faces = DirMask::empty();
        let mut saw_all = false;
        for f in faces_str.split([' ', ',']).filter(|f| !f.is_empty()) {
            match f.to_lowercase().as_str() {
                "all" => {
                    saw_all = true;
                    break;
                }
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
        if !saw_all {
            if custom_faces.is_empty() {
                return None;
            }
            faces = custom_faces;
        }
    }

    // 6. Weight / Priority
    let priority = props
        .get("weight")
        .and_then(|w| w.parse().ok())
        .unwrap_or(0);

    // 7. Biomes & Heights
    // `biomes=!a b` excludes the listed biomes (Continuity's `parseBiomes`).
    let (biomes, biomes_negated) = match props.get("biomes") {
        Some(raw) => {
            let trimmed = raw.trim();
            let (negated, list) = match trimmed.strip_prefix('!') {
                Some(rest) => (true, rest.trim()),
                None => (false, trimmed),
            };
            let entries: Vec<String> = list
                .split_whitespace()
                .map(|b| {
                    if b.contains(':') {
                        b.to_string()
                    } else {
                        format!("{}:{}", DEFAULT_NAMESPACE, b)
                    }
                })
                .collect();
            if entries.is_empty() && !negated {
                return None;
            }
            (Some(entries), negated)
        }
        None => (None, false),
    };

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
        inner_seams,
        biomes,
        biomes_negated,
        height_ranges,
        tint_index,
        tint_block,
    })
}
