use std::collections::HashMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

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

struct Signals {
    scores: HashMap<String, i32>,
    evidence: Vec<String>,
}

impl Signals {
    fn new() -> Self {
        Self {
            scores: HashMap::new(),
            evidence: Vec::new(),
        }
    }

    fn add(&mut self, kind: &str, weight: i32, reason: &str) {
        *self.scores.entry(kind.to_string()).or_insert(0) += weight;
        if weight > 0 {
            self.evidence.push(format!("{} (+{})", reason, weight));
        }
    }

    fn rank(&self) -> (String, i32, i32) {
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

fn strip_wrapper_root(names: &[String]) -> (String, Vec<String>) {
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

fn finalize(mut probe: PackProbe, sig: Signals) -> PackProbe {
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

fn extract_pack_mcmeta_details(
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

fn collect_dir_asset_paths(root: &Path, current: &Path, out: &mut Vec<String>, depth: usize) {
    if depth > 8 {
        return;
    }
    if let Ok(entries) = fs::read_dir(current) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(rel) = p.strip_prefix(root) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                if p.is_dir() {
                    collect_dir_asset_paths(root, &p, out, depth + 1);
                } else if p.is_file() {
                    out.push(rel_str);
                }
            }
        }
    }
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

fn classify_dir(path: &Path, mut probe: PackProbe) -> PackProbe {
    let read_res = fs::read_dir(path);
    if let Err(e) = read_res {
        probe.error = Some(e.to_string());
        return probe;
    }
    let entries_vec: Vec<_> = read_res.unwrap().flatten().collect();
    probe.entry_count = entries_vec.len();

    let mut sig = Signals::new();

    let is_file = |parts: &[&str]| -> bool {
        let mut p = path.to_path_buf();
        for part in parts {
            p.push(part);
        }
        p.is_file()
    };

    let is_dir = |parts: &[&str]| -> bool {
        let mut p = path.to_path_buf();
        for part in parts {
            p.push(part);
        }
        p.is_dir()
    };

    let read_json = |parts: &[&str]| -> Option<serde_json::Value> {
        let mut p = path.to_path_buf();
        for part in parts {
            p.push(part);
        }
        let f = File::open(p).ok()?;
        let mut buf = Vec::new();
        f.take(META_LIMIT).read_to_end(&mut buf).ok()?;
        serde_json::from_slice(&buf).ok()
    };

    let is_client_dir = is_dir(&["net", "minecraft", "client"]);

    if is_file(&["version.json"]) {
        sig.add(KIND_VANILLA_CLIENT, 30, "version.json");
        if is_client_dir {
            sig.add(KIND_VANILLA_CLIENT, 30, "net/minecraft/client");
        }
        if let Some(v) = read_json(&["version.json"]) {
            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mc_version".to_string(),
                    serde_json::Value::String(id.to_string()),
                );
            } else if let Some(name) = v.get("name").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mc_version".to_string(),
                    serde_json::Value::String(name.to_string()),
                );
            }
            if let Some(wv) = v.get("world_version") {
                probe
                    .details
                    .insert("world_version".to_string(), wv.clone());
            }
        }
    }

    if is_file(&["assets", ".mcassetsroot"]) {
        sig.add(KIND_VANILLA_CLIENT, 25, "assets/.mcassetsroot");
    }

    if is_dir(&["net", "minecraft", "bundler"])
        || (is_dir(&["net", "minecraft", "server"]) && !is_client_dir)
    {
        sig.add(KIND_VANILLA_SERVER, 30, "net/minecraft/bundler|server");
    }

    if is_file(&["fabric.mod.json"]) {
        sig.add(KIND_MOD, 40, "fabric.mod.json");
        if let Some(m) = read_json(&["fabric.mod.json"]) {
            if let Some(id) = m.get("id").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mod_id".to_string(),
                    serde_json::Value::String(id.to_string()),
                );
            }
            if let Some(name) = m.get("name").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mod_name".to_string(),
                    serde_json::Value::String(name.to_string()),
                );
            }
            if let Some(ver) = m.get("version").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mod_version".to_string(),
                    serde_json::Value::String(ver.to_string()),
                );
            }
        }
    }

    if is_file(&["quilt.mod.json"]) {
        sig.add(KIND_MOD, 40, "quilt.mod.json");
    }
    if is_file(&["META-INF", "mods.toml"]) {
        sig.add(KIND_MOD, 40, "META-INF/mods.toml");
    }
    if is_file(&["META-INF", "neoforge.mods.toml"]) {
        sig.add(KIND_MOD, 40, "META-INF/neoforge.mods.toml");
    }

    let has_assets_dir = is_dir(&["assets"]);

    if is_file(&["pack.mcmeta"]) {
        if let Some(meta) = read_json(&["pack.mcmeta"]) {
            extract_pack_mcmeta_details(&mut probe.details, &meta);
        }
        if has_assets_dir {
            sig.add(KIND_RESOURCE_PACK, 35, "pack.mcmeta + assets/");
        } else if is_dir(&["data"]) {
            sig.add(KIND_DATA_PACK, 35, "pack.mcmeta + data/ only");
        } else {
            sig.add(KIND_RESOURCE_PACK, 10, "pack.mcmeta only");
        }
    } else if has_assets_dir {
        sig.add(KIND_RESOURCE_PACK, 15, "assets/");
    }

    if is_file(&["pack.png"]) {
        sig.add(KIND_RESOURCE_PACK, 5, "pack.png");
    }

    if is_file(&["modrinth.index.json"]) {
        sig.add(KIND_MODPACK, 40, "modrinth.index.json");
    }

    let mut dir_asset_paths = Vec::new();
    let assets_dir = path.join("assets");
    if assets_dir.is_dir() {
        collect_dir_asset_paths(path, &assets_dir, &mut dir_asset_paths, 0);
    }
    let asset_summary = summarize_assets_from_paths(dir_asset_paths.iter().map(|s| s.as_str()));
    if let Ok(val) = serde_json::to_value(&asset_summary) {
        probe.details.insert("asset_summary".to_string(), val);
    }
    probe.asset_summary = asset_summary;

    finalize(probe, sig)
}

#[cfg(feature = "zip")]
fn classify_file(path: &Path, mut probe: PackProbe) -> PackProbe {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            probe.error = Some(e.to_string());
            return probe;
        }
    };

    let mut archive = match zip::ZipArchive::new(file) {
        Ok(a) => a,
        Err(zip::result::ZipError::InvalidArchive(e)) => {
            probe.error = Some(format!("not a readable zip/jar archive: {}", e));
            return probe;
        }
        Err(e) => {
            probe.error = Some(format!("not a readable zip/jar archive: {}", e));
            return probe;
        }
    };

    let raw_names: Vec<String> = archive
        .file_names()
        .filter_map(|r| r.ok())
        .map(|s| s.to_string())
        .collect();
    if raw_names.is_empty() {
        probe.error = Some("archive is empty".to_string());
        return probe;
    }

    let (prefix, names) = strip_wrapper_root(&raw_names);
    probe.entry_count = names.len();
    if !prefix.is_empty() {
        probe.details.insert(
            "archive_root".to_string(),
            serde_json::Value::String(prefix.trim_end_matches('/').to_string()),
        );
    }

    if names.is_empty() {
        probe.error = Some("archive is empty".to_string());
        return probe;
    }

    let names_set: std::collections::HashSet<&str> = names.iter().map(|s| s.as_str()).collect();

    let has = |name: &str| -> bool { names_set.contains(name) };
    let has_prefix = |pref: &str| -> bool { names.iter().any(|n| n.starts_with(pref)) };

    let mut sig = Signals::new();

    let read_entry_bytes =
        |archive: &mut zip::ZipArchive<File>, entry_name: &str| -> Option<Vec<u8>> {
            let full = if prefix.is_empty() {
                entry_name.to_string()
            } else {
                format!("{}{}", prefix, entry_name)
            };
            let entry = archive.by_name(&full).ok()?;
            let mut buf = Vec::new();
            entry.take(META_LIMIT).read_to_end(&mut buf).ok()?;
            Some(buf)
        };

    let read_entry_json =
        |archive: &mut zip::ZipArchive<File>, entry_name: &str| -> Option<serde_json::Value> {
            let buf = read_entry_bytes(archive, entry_name)?;
            serde_json::from_slice(&buf).ok()
        };

    let read_entry_text =
        |archive: &mut zip::ZipArchive<File>, entry_name: &str| -> Option<String> {
            let buf = read_entry_bytes(archive, entry_name)?;
            Some(String::from_utf8_lossy(&buf).to_string())
        };

    // 1. Vanilla client
    if has("version.json") {
        sig.add(KIND_VANILLA_CLIENT, 30, "root version.json");
        if let Some(v) = read_entry_json(&mut archive, "version.json") {
            if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mc_version".to_string(),
                    serde_json::Value::String(id.to_string()),
                );
            } else if let Some(name) = v.get("name").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mc_version".to_string(),
                    serde_json::Value::String(name.to_string()),
                );
            }
            if let Some(wv) = v.get("world_version") {
                probe
                    .details
                    .insert("world_version".to_string(), wv.clone());
            }
        }
    }

    let has_client_main = has("net/minecraft/client/main/Main.class");
    let has_client_class = has("net/minecraft/client/Minecraft.class");
    if has_client_main {
        sig.add(
            KIND_VANILLA_CLIENT,
            30,
            "net/minecraft/client/main/Main.class",
        );
    }
    if has_client_class {
        sig.add(
            KIND_VANILLA_CLIENT,
            20,
            "net/minecraft/client/Minecraft.class",
        );
    }
    if has("assets/.mcassetsroot") {
        sig.add(KIND_VANILLA_CLIENT, 25, "assets/.mcassetsroot");
    }
    if has("data/.mcassetsroot") {
        sig.add(KIND_VANILLA_CLIENT, 10, "data/.mcassetsroot");
    }
    if names.iter().any(|n| n.starts_with(MOJANG_SIGNATURE_PREFIX)) {
        sig.add(KIND_VANILLA_CLIENT, 25, "META-INF/MOJANGCS.* signature");
    }

    // 2. Vanilla server
    if has("META-INF/versions.list") && has("META-INF/libraries.list") {
        sig.add(KIND_VANILLA_SERVER, 40, "server bundler manifests");
    }
    if has("net/minecraft/bundler/Main.class") {
        sig.add(KIND_VANILLA_SERVER, 30, "net/minecraft/bundler/Main.class");
    }
    if has("net/minecraft/server/Main.class")
        && !(has_client_main || has_client_class || has("version.json"))
    {
        sig.add(KIND_VANILLA_SERVER, 25, "net/minecraft/server/Main.class");
    }

    // 3. Mod loaders
    if has("fabric.mod.json") {
        sig.add(KIND_MOD, 40, "fabric.mod.json");
        if let Some(m) = read_entry_json(&mut archive, "fabric.mod.json") {
            if let Some(id) = m.get("id").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mod_id".to_string(),
                    serde_json::Value::String(id.to_string()),
                );
            }
            if let Some(name) = m.get("name").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mod_name".to_string(),
                    serde_json::Value::String(name.to_string()),
                );
            }
            if let Some(ver) = m.get("version").and_then(|x| x.as_str()) {
                probe.details.insert(
                    "mod_version".to_string(),
                    serde_json::Value::String(ver.to_string()),
                );
            }
        }
    }
    if has("quilt.mod.json") {
        sig.add(KIND_MOD, 40, "quilt.mod.json");
    }
    if has("META-INF/mods.toml") {
        sig.add(KIND_MOD, 40, "META-INF/mods.toml");
    }
    if has("META-INF/neoforge.mods.toml") {
        sig.add(KIND_MOD, 40, "META-INF/neoforge.mods.toml");
    }
    if names
        .iter()
        .any(|n| n.starts_with("META-INF/jars/") && n.ends_with(".jar"))
    {
        sig.add(KIND_MOD, 20, "META-INF/jars/*.jar (jar-in-jar)");
    }
    if names
        .iter()
        .any(|n| n.ends_with(".mixins.json") && !n.contains('/'))
    {
        sig.add(KIND_MOD, 15, "root mixin configuration");
    }
    if has_prefix("META-INF/services/") {
        sig.add(KIND_MOD, 5, "META-INF/services");
    }

    if has("META-INF/MANIFEST.MF") {
        if let Some(manifest) = read_entry_text(&mut archive, "META-INF/MANIFEST.MF") {
            if manifest.contains("FMLModType") || manifest.contains("ModLoader") {
                sig.add(KIND_MOD, 30, "MANIFEST FMLModType");
            }
            if manifest.contains("Main-Class: net.minecraft") {
                sig.add(KIND_VANILLA_CLIENT, 20, "MANIFEST Main-Class net.minecraft");
            }
        }
    }

    // 4. Resource pack / data pack
    let has_pack_mcmeta = has("pack.mcmeta");
    let has_assets = has_prefix("assets/");
    let has_data = has_prefix("data/");
    let has_classes = has_prefix("net/") && names.iter().any(|n| n.ends_with(".class"));

    if has_pack_mcmeta {
        if let Some(meta) = read_entry_json(&mut archive, "pack.mcmeta") {
            extract_pack_mcmeta_details(&mut probe.details, &meta);
        }
        if has_assets {
            sig.add(KIND_RESOURCE_PACK, 35, "pack.mcmeta + assets/");
        } else if has_data {
            sig.add(KIND_DATA_PACK, 35, "pack.mcmeta + data/ only");
        } else {
            sig.add(KIND_RESOURCE_PACK, 10, "pack.mcmeta only");
        }
    } else if has_assets {
        sig.add(
            KIND_RESOURCE_PACK,
            15,
            "assets/ without mod or vanilla markers",
        );
    }

    if has("pack.png") && !has_classes {
        sig.add(KIND_RESOURCE_PACK, 10, "root pack.png");
    }
    if has("assets/minecraft/textures/") || has_prefix("assets/minecraft/textures/") {
        sig.add(KIND_RESOURCE_PACK, 5, "assets/minecraft/textures/");
    }

    // 5. Modpacks
    if has("modrinth.index.json") {
        sig.add(KIND_MODPACK, 40, "modrinth.index.json");
    }
    if has("manifest.json") && has_prefix("overrides/") {
        sig.add(KIND_MODPACK, 40, "manifest.json + overrides/");
    }

    // 6. Third-party library jars
    if has_classes && !has_pack_mcmeta {
        sig.add(KIND_MOD, 5, "compiled classes");
    }

    let asset_summary = summarize_assets_from_paths(names.iter().map(|s| s.as_str()));
    if let Ok(val) = serde_json::to_value(&asset_summary) {
        probe.details.insert("asset_summary".to_string(), val);
    }
    probe.asset_summary = asset_summary;

    finalize(probe, sig)
}

#[cfg(not(feature = "zip"))]
fn classify_file(_path: &Path, mut probe: PackProbe) -> PackProbe {
    probe.error = Some("ZIP feature not compiled in mtk-resource".to_string());
    probe
}

pub const CANONICAL_BASE_BLOCKS: [&str; 3] = ["stone", "dirt", "oak_planks"];

/// Validates whether a given path (unpacked folder or JAR/ZIP archive) can serve as
/// the canonical vanilla client foundation for the baking pipeline.
pub fn path_provides_vanilla_client(path: &Path) -> bool {
    if !path.exists() {
        return false;
    }
    if path.is_dir() {
        let mc = path.join("assets").join("minecraft");
        for block in CANONICAL_BASE_BLOCKS {
            if !mc
                .join("blockstates")
                .join(format!("{}.json", block))
                .is_file()
            {
                return false;
            }
            if !mc
                .join("models")
                .join("block")
                .join(format!("{}.json", block))
                .is_file()
            {
                return false;
            }
            if !mc
                .join("textures")
                .join("block")
                .join(format!("{}.png", block))
                .is_file()
            {
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
#[path = "probe_tests.rs"]
mod tests;
