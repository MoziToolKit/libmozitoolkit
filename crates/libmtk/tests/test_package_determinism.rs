//! Two bakes of the same resource stack must produce the same package payloads.
//!
//! The asset cache and the scene packages are compared, diffed and shipped as build artefacts.
//! `HashMap` iteration order is randomized per map instance, so any map that is serialized in
//! place makes those artefacts differ between runs even though the inputs are identical.

use libmtk::{precompile_all_assets, PrecompileConfig};
use mtk_resource::{MemoryPack, ResourcePackStack};
use std::fs;
use std::path::{Path, PathBuf};

fn png(color: [u8; 4]) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(16, 16, image::Rgba(color));
    let mut bytes = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut bytes);
    img.write_to(&mut cursor, image::ImageFormat::Png).unwrap();
    bytes
}

/// A stack wide enough that hash order actually differs between two maps.
fn build_stack() -> ResourcePackStack {
    let mut pack = MemoryPack::new("DeterminismPack");
    pack.insert(
        "assets/minecraft/textures/block/shared.png",
        png([90, 90, 90, 255]),
    );

    for i in 0..48 {
        let model = r##"{"textures":{"all":"minecraft:block/shared"},"elements":[{"from":[0,0,0],"to":[16,16,16],"faces":{"up":{"texture":"#all","cullface":"up"},"down":{"texture":"#all","cullface":"down"},"north":{"texture":"#all","cullface":"north"},"south":{"texture":"#all","cullface":"south"},"west":{"texture":"#all","cullface":"west"},"east":{"texture":"#all","cullface":"east"}}}]}"##;
        pack.insert(
            format!("assets/minecraft/models/block/det_{i:02}.json"),
            model.as_bytes().to_vec(),
        );
        let blockstate =
            format!(r#"{{"variants":{{"":{{"model":"minecraft:block/det_{i:02}"}}}}}}"#);
        pack.insert(
            format!("assets/minecraft/blockstates/det_{i:02}.json"),
            blockstate.into_bytes(),
        );
    }

    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(pack));
    stack
}

fn config(atlas_size: u32) -> PrecompileConfig {
    PrecompileConfig {
        max_atlas_width: atlas_size,
        max_atlas_height: atlas_size,
        atlas_category: "blocks".to_string(),
        compile_atlas: true,
        compile_models: true,
        num_threads: None,
        resolver_overrides: None,
    }
}

fn bake_once(stack: &ResourcePackStack, tag: &str, atlas_size: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mtk_determinism_{tag}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = fs::remove_dir_all(&dir);
    let result = precompile_all_assets(stack, &dir, &config(atlas_size)).expect("precompile");
    assert!(result.success);
    PathBuf::from(result.package_path)
}

/// Chunk type tags the writers emit. `chunk_ids` asserts it found exactly `header.chunk_count`
/// identifiers, so a new tag cannot silently shrink this list.
const CHUNK_TAGS: [[u8; 4]; 9] = [
    *b"META", *b"VOXL", *b"MODL", *b"ATLS", *b"BIOM", *b"CTMR", *b"ENTT", *b"COMP", *b"BANR",
];

/// Every chunk in the package, including payloads the host would only decode on demand.
fn chunk_ids(path: &Path) -> Vec<String> {
    let reader = mtk_package::MtkPackageReader::open_file(path).expect("open package");
    let mut ids: Vec<String> = CHUNK_TAGS
        .iter()
        .flat_map(|tag| reader.iter_chunk_identifiers(*tag))
        .map(|id| id.to_string())
        .collect();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len() as u32,
        reader.header().chunk_count,
        "chunk_ids missed a type tag: add it to CHUNK_TAGS"
    );
    ids
}

/// Payload bytes, not the container: the manifest carries a creation timestamp.
#[test]
fn identical_stacks_produce_identical_package_payloads() {
    let stack = build_stack();
    let first = bake_once(&stack, "a", 1024);
    let second = bake_once(&stack, "b", 1024);

    let first_ids = chunk_ids(&first);
    assert_eq!(
        first_ids,
        chunk_ids(&second),
        "the two bakes wrote different chunks"
    );
    assert!(
        first_ids.iter().any(|id| id == "biome/mapping"),
        "the fixture must exercise the biome mapping chunk"
    );

    let first_reader = mtk_package::MtkPackageReader::open_file(&first).expect("reader");
    let second_reader = mtk_package::MtkPackageReader::open_file(&second).expect("reader");

    for id in first_ids {
        let a = first_reader
            .read_chunk_decompressed(&id)
            .unwrap_or_else(|e| panic!("read {id} from first package: {e}"));
        let b = second_reader
            .read_chunk_decompressed(&id)
            .unwrap_or_else(|e| panic!("read {id} from second package: {e}"));
        assert!(
            a == b,
            "chunk `{id}` differs between two bakes of the same stack ({} vs {} bytes)",
            a.len(),
            b.len()
        );
    }

    let _ = fs::remove_dir_all(first.parent().unwrap_or(Path::new(".")));
    let _ = fs::remove_dir_all(second.parent().unwrap_or(Path::new(".")));
}

/// The real pack stack exercises paths the synthetic fixture cannot: 18k baked models, the
/// vanilla biome mapping and baked banner patterns. Only `created_at_epoch_secs` may differ.
#[test]
fn a_real_pack_stack_bakes_deterministically() {
    let Some(jar) = mtk_testkit::fabric_jar() else {
        mtk_testkit::skip("no Minecraft jar (set MTK_TEST_JAR or place ../26.2-Fabric.jar)");
        return;
    };
    let mut stack = ResourcePackStack::new();
    stack.push_pack(Box::new(
        mtk_resource::ZipPack::from_file("vanilla", &jar).expect("open jar"),
    ));
    if let Some(assets) = mtk_testkit::real_assets_root() {
        stack.push_pack(Box::new(mtk_resource::DirectoryPack::new(
            "mojang_assets",
            &assets,
        )));
    }
    if let Some(pack) = mtk_testkit::resource_pack_zip() {
        if let Ok(zip) = mtk_resource::ZipPack::from_file("spbr", &pack) {
            stack.push_pack(Box::new(zip));
        }
    }

    // HD animated sprites (e.g. water_flow at 36x1028) do not fit the small fixture size.
    let first = bake_once(&stack, "real_a", 4096);
    let second = bake_once(&stack, "real_b", 4096);
    let first_ids = chunk_ids(&first);
    assert_eq!(first_ids, chunk_ids(&second));

    let first_reader = mtk_package::MtkPackageReader::open_file(&first).expect("reader");
    let second_reader = mtk_package::MtkPackageReader::open_file(&second).expect("reader");
    for id in first_ids {
        let a = first_reader.read_chunk_decompressed(&id).expect("chunk");
        let b = second_reader.read_chunk_decompressed(&id).expect("chunk");
        if id == "manifest" {
            continue; // creation timestamp is not an input
        }
        assert!(a == b, "chunk `{id}` differs between two real bakes");
    }

    let _ = fs::remove_dir_all(first.parent().unwrap_or(Path::new(".")));
    let _ = fs::remove_dir_all(second.parent().unwrap_or(Path::new(".")));
}
