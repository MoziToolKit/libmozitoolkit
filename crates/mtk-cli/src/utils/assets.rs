//! Asset access for the CLI, on top of the hardened resource-pack API.
//!
//! The CLI used to carry its own zip/directory reader (`UniversalAssetLoader`), which bypassed
//! the `mtk-resource` protections (per-entry size cap, path-traversal rejection, pack layering,
//! `.mcmeta` handling) and could disagree with what production prebaking actually reads. This
//! module is deliberately thin: it only adds JSON parsing and memoisation on top of
//! [`ResourcePackStack`], so there is exactly one asset reader in the workspace.

use std::collections::HashMap;
use std::path::Path;

use mtk_model::{BlockModelJson, BlockStateDefinition};
use mtk_resource::{DirectoryPack, ResourceError, ResourcePackStack, ZipPack};

/// Read failures raised inside `Option`-returning bake callbacks.
///
/// `ModelBaker`'s parent lookup callback cannot return an error, so an unreadable model would
/// otherwise be indistinguishable from a missing one. The first failure is parked here and
/// surfaced right after the bake.
#[derive(Default)]
pub struct ReadFailureCell(std::cell::RefCell<Option<ResourceError>>);

impl ReadFailureCell {
    /// Records the failure (if any) and forwards the value.
    pub fn capture<T>(&self, result: Result<Option<T>, ResourceError>) -> Option<T> {
        match result {
            Ok(value) => value,
            Err(err) => {
                let mut slot = self.0.borrow_mut();
                if slot.is_none() {
                    *slot = Some(err);
                }
                None
            }
        }
    }

    /// Fails with the first recorded read error.
    pub fn into_result(self) -> Result<(), ResourceError> {
        match self.0.into_inner() {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }
}

/// A resource pack (directory, `.zip` or `.jar`) opened for read-only CLI inspection.
pub struct AssetStack {
    stack: ResourcePackStack,
    blockstates: Vec<String>,
    model_cache: HashMap<String, Option<BlockModelJson>>,
    blockstate_cache: HashMap<String, Option<BlockStateDefinition>>,
}

impl AssetStack {
    /// Opens a directory, `.zip` or `.jar` pack through the hardened reader.
    pub fn new(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let path = path.as_ref();
        let mut stack = ResourcePackStack::new();
        if path.is_dir() {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("cli_pack");
            stack.push_pack(Box::new(DirectoryPack::new(name, path)));
        } else {
            let name = path
                .file_stem()
                .and_then(|name| name.to_str())
                .unwrap_or("cli_pack");
            stack.push_pack(Box::new(ZipPack::from_file(name, path)?));
        }

        // Deterministic order: the stack lists packs in priority order, but each pack enumerates
        // its entries from a hash map.
        let mut blockstates: Vec<String> = stack
            .list_all_blockstate_locations()
            .iter()
            .map(|location| location.as_string())
            .collect();
        blockstates.sort();
        blockstates.dedup();

        Ok(Self {
            stack,
            blockstates,
            model_cache: HashMap::new(),
            blockstate_cache: HashMap::new(),
        })
    }

    /// Every blockstate id in the pack, sorted (`minecraft:oak_stairs`, `mymod:thing`, ...).
    pub fn list_all_blockstates(&self) -> Vec<String> {
        self.blockstates.clone()
    }

    /// Reads raw bytes by explicit asset path (`assets/<ns>/<category>/<name>.<ext>`).
    ///
    /// A path that is absent yields `Ok(None)`; a file this pack holds but cannot read yields
    /// `Err`, so the CLI reports it instead of probing other packs.
    pub fn read_entry_bytes(
        &mut self,
        relative_path: &str,
    ) -> Result<Option<Vec<u8>>, ResourceError> {
        self.stack.open_asset_raw(relative_path)
    }

    /// Parses `assets/<ns>/blockstates/<name>.json`, memoised.
    pub fn load_blockstate(
        &mut self,
        blockstate_id: &str,
    ) -> Result<Option<BlockStateDefinition>, ResourceError> {
        if let Some(cached) = self.blockstate_cache.get(blockstate_id) {
            return Ok(cached.clone());
        }

        let (namespace, name) = blockstate_id
            .split_once(':')
            .unwrap_or(("minecraft", blockstate_id));
        let path = format!("assets/{namespace}/blockstates/{name}.json");
        let parsed = self
            .stack
            .open_asset_raw(&path)?
            .and_then(|bytes| serde_json::from_slice::<BlockStateDefinition>(&bytes).ok());

        self.blockstate_cache
            .insert(blockstate_id.to_string(), parsed.clone());
        Ok(parsed)
    }

    /// Parses a model JSON through `mtk-resource`'s canonical candidate paths, memoised.
    pub fn load_model(&mut self, model_id: &str) -> Result<Option<BlockModelJson>, ResourceError> {
        if let Some(cached) = self.model_cache.get(model_id) {
            return Ok(cached.clone());
        }

        let parsed = self
            .stack
            .open_model_raw(model_id)?
            .and_then(|bytes| serde_json::from_slice::<BlockModelJson>(&bytes).ok());

        self.model_cache
            .insert(model_id.to_string(), parsed.clone());
        Ok(parsed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_file(root: &Path, relative: &str, body: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, body).expect("write");
    }

    fn sample_pack(root: &Path) {
        write_file(
            root,
            "assets/minecraft/blockstates/oak_stairs.json",
            r#"{"variants":{"facing=north,half=bottom,shape=straight":{"model":"minecraft:block/oak_stairs"}}}"#,
        );
        write_file(
            root,
            "assets/mymod/blockstates/gadget.json",
            r#"{"variants":{"":{"model":"mymod:block/gadget"}}}"#,
        );
        write_file(
            root,
            "assets/minecraft/models/block/oak_stairs.json",
            r#"{"parent":"block/cube","textures":{"all":"block/oak_planks"}}"#,
        );
        write_file(
            root,
            "assets/mymod/models/block/gadget.json",
            r#"{"textures":{"all":"mymod:block/gadget_tex"}}"#,
        );
        write_file(
            root,
            "assets/minecraft/textures/block/oak_planks.png",
            "PNG",
        );
    }

    fn assert_pack_resolves(stack: &mut AssetStack) {
        assert_eq!(
            stack.list_all_blockstates(),
            vec![
                "minecraft:oak_stairs".to_string(),
                "mymod:gadget".to_string()
            ],
            "blockstates must be namespaced, sorted and deduplicated"
        );

        let stairs = stack
            .load_blockstate("minecraft:oak_stairs")
            .expect("read")
            .expect("blockstate must parse");
        assert!(stairs.variants.is_some());

        // Bare and namespaced ids both resolve through the canonical candidate paths.
        assert!(stack
            .load_model("minecraft:block/oak_stairs")
            .unwrap()
            .is_some());
        assert!(stack.load_model("block/oak_stairs").unwrap().is_some());
        assert!(stack.load_model("mymod:block/gadget").unwrap().is_some());
        assert!(stack.load_model("mymod:block/missing").unwrap().is_none());

        assert_eq!(
            stack
                .read_entry_bytes("assets/minecraft/textures/block/oak_planks.png")
                .unwrap(),
            Some(b"PNG".to_vec())
        );
    }

    #[test]
    fn directory_pack_lists_and_resolves_assets() {
        let tmp = tempfile::tempdir().expect("tempdir");
        sample_pack(tmp.path());
        let mut stack = AssetStack::new(tmp.path()).expect("open directory pack");
        assert_pack_resolves(&mut stack);
    }

    #[test]
    fn archive_pack_matches_the_directory_pack() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let source = tmp.path().join("src");
        sample_pack(&source);

        let archive_path = tmp.path().join("pack.zip");
        {
            let file = std::fs::File::create(&archive_path).expect("zip file");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            let mut entries: Vec<_> = walk(&source)
                .into_iter()
                .map(|path| {
                    (
                        path.strip_prefix(&source)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                        std::fs::read(&path).expect("read"),
                    )
                })
                .collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            for (name, bytes) in entries {
                zip.start_file(name, options).expect("start_file");
                zip.write_all(&bytes).expect("write");
            }
            zip.finish().expect("finish");
        }

        let mut stack = AssetStack::new(&archive_path).expect("open archive pack");
        assert_pack_resolves(&mut stack);
    }

    fn walk(root: &Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).expect("read_dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    out.push(path);
                }
            }
        }
        out
    }

    /// The CLI used to hand-roll its zip reader, which happily exposed `../` entries. It now goes
    /// through `mtk-resource`, which skips them; this pins that the CLI cannot read outside the
    /// pack even if a hostile archive tries.
    #[test]
    fn path_traversal_entries_are_not_reachable() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let archive_path = tmp.path().join("evil.zip");
        {
            let file = std::fs::File::create(&archive_path).expect("zip file");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for name in [
                "../escape.json",
                "assets/../../escape.json",
                "/etc/escape.json",
            ] {
                zip.start_file(name, options).expect("start_file");
                zip.write_all(b"{}").expect("write");
            }
            zip.finish().expect("finish");
        }

        let mut stack = AssetStack::new(&archive_path).expect("open hostile archive");
        for probe in [
            "../escape.json",
            "assets/../../escape.json",
            "/etc/escape.json",
        ] {
            assert!(
                stack.read_entry_bytes(probe).unwrap().is_none(),
                "`{probe}` must not be readable through the pack"
            );
        }
        assert!(stack.list_all_blockstates().is_empty());
    }
}
