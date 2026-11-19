//! Decorated Pot specific normalization and sherd texture mappings.

/// Normalizes a Minecraft Pottery Sherd item ID or pattern name into a standard pattern key.
///
/// E.g. `"minecraft:angler_pottery_sherd"` -> `Some("angler")`
///      `"minecraft:brick"` -> `None`
pub fn normalize_sherd_pattern(item_id: &str) -> Option<&'static str> {
    let clean = item_id.strip_prefix("minecraft:").unwrap_or(item_id);
    let stem = clean
        .strip_suffix("_pottery_sherd")
        .or_else(|| clean.strip_suffix("_pottery_pattern"))
        .unwrap_or(clean);

    match stem {
        "angler" => Some("angler"),
        "archer" => Some("archer"),
        "arms_up" => Some("arms_up"),
        "blade" => Some("blade"),
        "brewer" => Some("brewer"),
        "burn" => Some("burn"),
        "danger" => Some("danger"),
        "explorer" => Some("explorer"),
        "flow" => Some("flow"),
        "friend" => Some("friend"),
        "guster" => Some("guster"),
        "heart" => Some("heart"),
        "heartbreak" => Some("heartbreak"),
        "howl" => Some("howl"),
        "miner" => Some("miner"),
        "mourner" => Some("mourner"),
        "plenty" => Some("plenty"),
        "prize" => Some("prize"),
        "scrape" => Some("scrape"),
        "sheaf" => Some("sheaf"),
        "shelter" => Some("shelter"),
        "skull" => Some("skull"),
        "snort" => Some("snort"),
        "brick" | "blank" | "" => None,
        _ => None,
    }
}

/// Resolves the static texture identifier for a specific sherd side without heap allocation.
pub fn sherd_texture_ref(pattern: Option<&str>) -> &'static str {
    match pattern.and_then(normalize_sherd_pattern) {
        Some("angler") => "minecraft:entity/decorated_pot/angler_pottery_pattern",
        Some("archer") => "minecraft:entity/decorated_pot/archer_pottery_pattern",
        Some("arms_up") => "minecraft:entity/decorated_pot/arms_up_pottery_pattern",
        Some("blade") => "minecraft:entity/decorated_pot/blade_pottery_pattern",
        Some("brewer") => "minecraft:entity/decorated_pot/brewer_pottery_pattern",
        Some("burn") => "minecraft:entity/decorated_pot/burn_pottery_pattern",
        Some("danger") => "minecraft:entity/decorated_pot/danger_pottery_pattern",
        Some("explorer") => "minecraft:entity/decorated_pot/explorer_pottery_pattern",
        Some("flow") => "minecraft:entity/decorated_pot/flow_pottery_pattern",
        Some("friend") => "minecraft:entity/decorated_pot/friend_pottery_pattern",
        Some("guster") => "minecraft:entity/decorated_pot/guster_pottery_pattern",
        Some("heart") => "minecraft:entity/decorated_pot/heart_pottery_pattern",
        Some("heartbreak") => "minecraft:entity/decorated_pot/heartbreak_pottery_pattern",
        Some("howl") => "minecraft:entity/decorated_pot/howl_pottery_pattern",
        Some("miner") => "minecraft:entity/decorated_pot/miner_pottery_pattern",
        Some("mourner") => "minecraft:entity/decorated_pot/mourner_pottery_pattern",
        Some("plenty") => "minecraft:entity/decorated_pot/plenty_pottery_pattern",
        Some("prize") => "minecraft:entity/decorated_pot/prize_pottery_pattern",
        Some("scrape") => "minecraft:entity/decorated_pot/scrape_pottery_pattern",
        Some("sheaf") => "minecraft:entity/decorated_pot/sheaf_pottery_pattern",
        Some("shelter") => "minecraft:entity/decorated_pot/shelter_pottery_pattern",
        Some("skull") => "minecraft:entity/decorated_pot/skull_pottery_pattern",
        Some("snort") => "minecraft:entity/decorated_pot/snort_pottery_pattern",
        _ => "minecraft:entity/decorated_pot/decorated_pot_side",
    }
}

/// Resolves the texture identifier for a specific sherd side.
#[inline]
pub fn sherd_texture(pattern: Option<&str>) -> String {
    sherd_texture_ref(pattern).to_string()
}
