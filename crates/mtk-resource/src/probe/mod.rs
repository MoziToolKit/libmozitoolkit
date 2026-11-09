use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

mod classify_dir;
mod classify_file;

use classify_dir::classify_dir;
use classify_file::classify_file;

pub const KIND_VANILLA_CLIENT: &str = "VANILLA_CLIENT";
pub const KIND_VANILLA_SERVER: &str = "VANILLA_SERVER";
pub const KIND_MOD: &str = "MOD";
pub const KIND_RESOURCE_PACK: &str = "RESOURCE_PACK";
pub const KIND_DATA_PACK: &str = "DATA_PACK";
pub const KIND_MODPACK: &str = "MODPACK";
pub const KIND_UNKNOWN: &str = "UNKNOWN";

pub const META_LIMIT: u64 = 64 * 1024;
pub const MIN_SCORE: i32 = 15;
pub const MIN_CONFIDENCE: f64 = 0.15;

const METADATA_PROBES: [&str; 6] = [
    "pack.mcmeta",
    "version.json",
    "fabric.mod.json",
    "quilt.mod.json",
    "META-INF/mods.toml",
    "META-INF/neoforge.mods.toml",
];

const MOJANG_SIGNATURE_PREFIX: &str = "META-INF/MOJANGCS.";

#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct AssetSummary {
    pub has_renderable_assets: bool,
    pub has_block_assets: bool,
    pub blockstate_count: usize,
    pub block_model_count: usize,
    pub item_model_count: usize,
    pub block_texture_count: usize,
    pub ctm_property_count: usize,
    pub total_asset_entries: usize,
    pub namespaces: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PackProbe {
    pub kind: String,
    pub confidence: f64,
    pub suggested_pack_type: Option<String>,
    pub evidence: Vec<String>,
    pub details: HashMap<String, serde_json::Value>,
    pub error: Option<String>,
    pub entry_count: usize,
    pub asset_summary: AssetSummary,
}

impl Default for PackProbe {
    fn default() -> Self {
        Self {
            kind: KIND_UNKNOWN.to_string(),
            confidence: 0.0,
            suggested_pack_type: None,
            evidence: Vec::new(),
            details: HashMap::new(),
            error: None,
            entry_count: 0,
            asset_summary: AssetSummary::default(),
        }
    }
}

pub(crate) struct Signals {
    pub(crate) scores: HashMap<String, i32>,
    pub(crate) evidence: Vec<String>,
}

impl Signals {
    pub(crate) fn new() -> Self {
        Self {
            scores: HashMap::new(),
            evidence: Vec::new(),
        }
    }

    pub(crate) fn add(&mut self, kind: &str, weight: i32, reason: &str) {
        *self.scores.entry(kind.to_string()).or_insert(0) += weight;
        if weight > 0 {
            self.evidence.push(format!("{} (+{})", reason, weight));
        }
    }

    pub(crate) fn rank(&self) -> (String, i32, i32) {
        if self.scores.is_empty() {
            return (KIND_UNKNOWN.to_string(), 0, 0);
        }
        let mut ordered: Vec<(&String, &i32)> = self.scores.iter().collect();
        ordered.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        let (top_kind, top_score) = ordered[0];
        let second_score = if ordered.len() > 1 { *ordered[1].1 } else { 0 };
        (top_kind.clone(), *top_score, second_score)
    }
}

pub(crate) fn strip_wrapper_root(names: &[String]) -> (String, Vec<String>) {
    let files: Vec<&str> = names
        .iter()
        .filter(|n| !n.is_empty() && !n.ends_with('/'))
        .map(|s| s.as_str())
        .collect();

    if files.is_empty() {
        return (String::new(), names.to_vec());
    }
    if files.iter().any(|n| !n.contains('/')) {
        return (String::new(), names.to_vec());
    }

    let mut heads = std::collections::HashSet::new();
    for f in &files {
        if let Some(h) = f.split('/').next() {
            heads.insert(h);
        }
    }
    if heads.len() != 1 {
        return (String::new(), names.to_vec());
    }
    let head = heads.into_iter().next().unwrap();
    let prefix = format!("{}/", head);

    let has_probe = METADATA_PROBES
        .iter()
        .any(|p| names.iter().any(|n| n == &format!("{}{}", prefix, p)));
    if !has_probe {
        return (String::new(), names.to_vec());
    }

    let stripped = names
        .iter()
        .filter(|n| n.starts_with(&prefix))
        .map(|n| n[prefix.len()..].to_string())
        .collect();

    (prefix, stripped)
}

pub(crate) fn finalize(mut probe: PackProbe, sig: Signals) -> PackProbe {
    let (kind, top, second) = sig.rank();
    probe.evidence = sig.evidence;
    let scores_json = serde_json::to_value(&sig.scores).unwrap_or(serde_json::Value::Null);
    probe
        .details
        .entry("scores".to_string())
        .or_insert(scores_json);

    if top < MIN_SCORE {
        probe.kind = KIND_UNKNOWN.to_string();
        probe.confidence = 0.0;
        probe.suggested_pack_type = None;
        return probe;
    }

    probe.kind = kind.clone();
    probe.confidence = if top <= 0 {
        1.0
    } else {
        ((top - second) as f64 / top as f64).clamp(0.0, 1.0)
    };

    let suggestion = match kind.as_str() {
        KIND_VANILLA_CLIENT | KIND_VANILLA_SERVER => Some("VANILLA".to_string()),
        KIND_MOD => Some("MOD_JAR".to_string()),
        KIND_RESOURCE_PACK => Some("RESOURCE_PACK".to_string()),
        _ => None,
    };

    if suggestion.is_none() || probe.confidence < MIN_CONFIDENCE {
        probe.suggested_pack_type = None;
    } else {
        probe.suggested_pack_type = suggestion;
    }

    probe
}

pub(crate) fn extract_pack_mcmeta_details(
    details: &mut HashMap<String, serde_json::Value>,
    meta: &serde_json::Value,
) {
    if let Some(pack) = meta.get("pack") {
        if let Some(fmt) = pack.get("pack_format") {
            details.insert("pack_format".to_string(), fmt.clone());
        }
        if let Some(desc) = pack.get("description") {
            if let Some(desc_str) = desc.as_str() {
                let truncated: String = desc_str.chars().take(160).collect();
                details.insert(
                    "description".to_string(),
                    serde_json::Value::String(truncated),
                );
            } else {
                details.insert("description".to_string(), desc.clone());
            }
        }
    }
}

pub(crate) fn summarize_assets_from_paths<'a, I>(entries: I) -> AssetSummary
where
    I: IntoIterator<Item = &'a str>,
{
    let mut summary = AssetSummary::default();
    let mut ns_set = std::collections::BTreeSet::new();

    for raw in entries {
        let entry = raw.trim_start_matches('/');
        if !entry.starts_with("assets/") {
            continue;
        }
        summary.total_asset_entries += 1;

        let rest = &entry["assets/".len()..];
        if let Some(slash_pos) = rest.find('/') {
            let ns = &rest[..slash_pos];
            if !ns.is_empty() && ns != ".mcassetsroot" {
                ns_set.insert(ns.to_string());
            }
            let sub = &rest[slash_pos + 1..];

            if sub.starts_with("blockstates/") && sub.ends_with(".json") {
                summary.blockstate_count += 1;
            } else if sub.starts_with("models/block/") && sub.ends_with(".json") {
                summary.block_model_count += 1;
            } else if sub.starts_with("models/item/") && sub.ends_with(".json") {
                summary.item_model_count += 1;
            } else if sub.starts_with("textures/block/") && sub.ends_with(".png") {
                summary.block_texture_count += 1;
            } else if (sub.starts_with("optifine/") || sub.starts_with("textures/"))
                && sub.ends_with(".properties")
            {
                summary.ctm_property_count += 1;
            }
        }
    }

    summary.has_block_assets = summary.blockstate_count > 0
        || summary.block_model_count > 0
        || summary.block_texture_count > 0
        || summary.ctm_property_count > 0;

    summary.has_renderable_assets = summary.has_block_assets || summary.item_model_count > 0;
    summary.namespaces = ns_set.into_iter().collect();
    summary
}

pub fn probe_pack(path: &Path) -> PackProbe {
    let mut probe = PackProbe::default();

    if !path.exists() {
        probe.error = Some("path does not exist".to_string());
        return probe;
    }

    if path.is_dir() {
        classify_dir(path, probe)
    } else if path.is_file() {
        classify_file(path, probe)
    } else {
        probe.error = Some("invalid path".to_string());
        probe
    }
}

pub const CANONICAL_BASE_BLOCKS: [&str; 3] = ["stone", "dirt", "oak_planks"];

pub fn path_provides_vanilla_client(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }

    if path.is_dir() {
        let is_dir = |parts: &[&str]| -> bool {
            let mut p = path.to_path_buf();
            for part in parts {
                p.push(part);
            }
            p.is_dir()
        };

        let is_client_dir = is_dir(&["net", "minecraft", "client"]);
        if is_client_dir {
            return true;
        }

        for block in CANONICAL_BASE_BLOCKS {
            let bs = path
                .join("assets")
                .join("minecraft")
                .join("blockstates")
                .join(format!("{}.json", block));
            let model = path
                .join("assets")
                .join("minecraft")
                .join("models")
                .join("block")
                .join(format!("{}.json", block));
            let tex = path
                .join("assets")
                .join("minecraft")
                .join("textures")
                .join("block")
                .join(format!("{}.png", block));
            if !bs.is_file() || !model.is_file() || !tex.is_file() {
                return false;
            }
        }
        true
    } else if path.is_file() {
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if ext != "jar" && ext != "zip" {
            return false;
        }
        let probe = probe_pack(path);
        if probe.kind == KIND_VANILLA_CLIENT {
            return true;
        }
        #[cfg(feature = "zip")]
        {
            let file = match File::open(path) {
                Ok(f) => f,
                Err(_) => return false,
            };
            let archive = match zip::ZipArchive::new(file) {
                Ok(a) => a,
                Err(_) => return false,
            };
            let raw_names: Vec<String> = archive
                .file_names()
                .filter_map(|r| r.ok())
                .map(|s| s.to_string())
                .collect();
            let (_, names) = strip_wrapper_root(&raw_names);
            let names_set: std::collections::HashSet<&str> =
                names.iter().map(|s| s.as_str()).collect();

            for block in CANONICAL_BASE_BLOCKS {
                let bs = format!("assets/minecraft/blockstates/{}.json", block);
                let model = format!("assets/minecraft/models/block/{}.json", block);
                let tex = format!("assets/minecraft/textures/block/{}.png", block);
                if !names_set.contains(bs.as_str())
                    || !names_set.contains(model.as_str())
                    || !names_set.contains(tex.as_str())
                {
                    return false;
                }
            }
            true
        }
        #[cfg(not(feature = "zip"))]
        {
            false
        }
    } else {
        false
    }
}

#[cfg(test)]
#[path = "../probe_tests.rs"]
mod tests;
