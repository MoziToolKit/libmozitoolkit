use std::fs::File;
use std::io::Read;
use std::path::Path;

use super::{
    extract_pack_mcmeta_details, finalize, strip_wrapper_root, summarize_assets_from_paths,
    PackProbe, Signals, KIND_DATA_PACK, KIND_MOD, KIND_MODPACK, KIND_RESOURCE_PACK,
    KIND_VANILLA_CLIENT, KIND_VANILLA_SERVER, META_LIMIT, MOJANG_SIGNATURE_PREFIX,
};

#[cfg(feature = "zip")]
pub(crate) fn classify_file(path: &Path, mut probe: PackProbe) -> PackProbe {
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
            let bytes = read_entry_bytes(archive, entry_name)?;
            serde_json::from_slice(&bytes).ok()
        };

    let read_entry_text =
        |archive: &mut zip::ZipArchive<File>, entry_name: &str| -> Option<String> {
            let buf = read_entry_bytes(archive, entry_name)?;
            Some(String::from_utf8_lossy(&buf).to_string())
        };

    let is_client_jar = has_prefix("net/minecraft/client/");

    if has("version.json") {
        sig.add(KIND_VANILLA_CLIENT, 30, "version.json");
        if is_client_jar {
            sig.add(KIND_VANILLA_CLIENT, 30, "net/minecraft/client");
        }
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

    if has("assets/.mcassetsroot") {
        sig.add(KIND_VANILLA_CLIENT, 25, "assets/.mcassetsroot");
    }

    if has_prefix(MOJANG_SIGNATURE_PREFIX) {
        sig.add(KIND_VANILLA_CLIENT, 15, "META-INF/MOJANGCS.* signature");
    }

    if has_prefix("net/minecraft/bundler/")
        || (has_prefix("net/minecraft/server/") && !is_client_jar)
    {
        sig.add(KIND_VANILLA_SERVER, 30, "net/minecraft/bundler|server");
    }
    if has("META-INF/versions.list") {
        sig.add(KIND_VANILLA_SERVER, 25, "META-INF/versions.list");
    }
    if has("META-INF/libraries.list") {
        sig.add(KIND_VANILLA_SERVER, 15, "META-INF/libraries.list");
    }

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
    if has("mcmod.info") {
        sig.add(KIND_MOD, 30, "mcmod.info (legacy forge)");
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

    if has("modrinth.index.json") {
        sig.add(KIND_MODPACK, 40, "modrinth.index.json");
    }
    if has("manifest.json") && has_prefix("overrides/") {
        sig.add(KIND_MODPACK, 40, "manifest.json + overrides/");
    } else if has("manifest.json") {
        sig.add(KIND_MODPACK, 40, "manifest.json");
    }
    if has("mcmeta.json") {
        sig.add(KIND_MODPACK, 30, "mcmeta.json");
    }

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
pub(crate) fn classify_file(_path: &Path, mut probe: PackProbe) -> PackProbe {
    probe.error = Some("ZIP feature not compiled in mtk-resource".to_string());
    probe
}
