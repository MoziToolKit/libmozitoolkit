use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use super::{
    extract_pack_mcmeta_details, finalize, summarize_assets_from_paths, PackProbe, Signals,
    KIND_DATA_PACK, KIND_MOD, KIND_MODPACK, KIND_RESOURCE_PACK, KIND_VANILLA_CLIENT,
    KIND_VANILLA_SERVER, META_LIMIT,
};

pub(crate) fn collect_dir_asset_paths(
    root: &Path,
    current: &Path,
    out: &mut Vec<String>,
    depth: usize,
) {
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

pub(crate) fn classify_dir(path: &Path, mut probe: PackProbe) -> PackProbe {
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
