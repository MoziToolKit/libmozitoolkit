#![forbid(unsafe_code)]
//! # mtk-testkit
//!
//! Dev-only asset resolver shared by the `libmozitoolkit` integration tests.
//!
//! Real Minecraft resource packs / saves are large and licensed, so they are **never**
//! committed to the repository. Instead this crate resolves them at test time using a
//! deterministic priority chain:
//!
//! 1. Environment variable overrides (set by CI, see `.github/workflows/ci.yml`).
//! 2. Conventional local paths adjacent to the workspace (`../mc`, `../26.2-Fabric.jar`, ...).
//! 3. A minimal set of vendored JSON fixtures shipped inside this crate (`fixtures/mc`).
//!
//! Tests that require the *full* vanilla asset set (e.g. canonical debug-world generation)
//! should query [`real_assets_root`] and skip gracefully when it returns `None`, while
//! hermetic tests can use [`assets_root`], which always falls back to the vendored fixtures.
//!
//! ## Environment variables
//!
//! | Variable | Purpose |
//! | :--- | :--- |
//! | `MTK_TEST_ASSETS` / `MC_ASSETS_DIR` / `MC_DIR` | Unpacked Minecraft jar root (contains `assets/minecraft`) |
//! | `MTK_TEST_JAR` / `MC_JAR` | Minecraft client jar for pack-level baking tests |
//! | `MTK_TEST_RESOURCE_PACK` | External resource pack `.zip` (e.g. SPBR / PBR packs) |
//! | `MTK_TEST_SAVE` | Minecraft save directory (contains `level.dat`) |
//! | `MTK_TEST_MODELS_CACHE` | Pre-built `models.bin` cache for serialization benchmarks |
//! | `MTK_TEST_ONLINE` / `MTK_ONLINE_TESTS` | Explicitly opt in to live HTTP/network tests |

use std::path::{Path, PathBuf};

/// Directory name of the vendored minimal fixtures shipped with this crate.
const FIXTURES_DIR: &str = "fixtures";

/// Root of the `libmozitoolkit` workspace.
///
/// Resolved relative to this crate's manifest, which is always `<workspace>/crates/mtk-testkit`.
pub fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("mtk-testkit must live at <workspace>/crates/mtk-testkit")
        .to_path_buf()
}

/// Root of the vendored fixtures bundled with this crate.
pub fn fixtures_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(FIXTURES_DIR)
}

fn env_existing(keys: &[&str]) -> Option<PathBuf> {
    keys.iter().find_map(|key| {
        std::env::var_os(key)
            .map(PathBuf::from)
            .filter(|path| path.exists())
    })
}

fn first_existing<I>(candidates: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    candidates.into_iter().find(|path| path.exists())
}

fn is_asset_root(path: &Path) -> bool {
    path.join("assets").join("minecraft").is_dir()
}

/// Real (full) Minecraft asset root, if present on this machine.
///
/// Returns `None` for hermetic environments that only ship the vendored fixtures.
pub fn real_assets_root() -> Option<PathBuf> {
    if let Some(path) = env_existing(&["MTK_TEST_ASSETS", "MC_ASSETS_DIR", "MC_DIR"]) {
        if is_asset_root(&path) {
            return Some(path);
        }
    }

    let workspace = workspace_root();
    first_existing([workspace.join("../mc"), workspace.join("mc")])
        .filter(|path| is_asset_root(path))
}

/// Any usable asset root: real assets when available, otherwise vendored fixtures.
///
/// Always prefer [`real_assets_root`] for tests that assert on vanilla completeness.
pub fn assets_root() -> Option<PathBuf> {
    if let Some(path) = real_assets_root() {
        return Some(path);
    }

    let fixtures = fixtures_root().join("mc");
    is_asset_root(&fixtures).then_some(fixtures)
}

/// `true` when the full vanilla asset set is available (not just the vendored fixtures).
pub fn has_real_assets() -> bool {
    real_assets_root().is_some()
}

/// Best-effort user home directory (`HOME` on Unix, `USERPROFILE` on Windows).
fn home_dir() -> Option<PathBuf> {
    ["HOME", "USERPROFILE"]
        .iter()
        .find_map(|key| std::env::var_os(key).map(PathBuf::from))
}

/// Minecraft client jar used for end-to-end resource-pack baking tests.
pub fn fabric_jar() -> Option<PathBuf> {
    let workspace = workspace_root();
    let mut candidates = vec![
        workspace.join("../26.2-Fabric.jar"),
        workspace.join("../fabric.jar"),
    ];
    if let Some(home) = home_dir() {
        candidates.push(home.join("26.2-Fabric.jar"));
    }

    env_existing(&["MTK_TEST_JAR", "MC_JAR"]).or_else(|| first_existing(candidates))
}

/// External resource pack `.zip` (e.g. SPBR / Continuity / PBR companion packs).
pub fn resource_pack_zip() -> Option<PathBuf> {
    let workspace = workspace_root();
    let mut candidates = vec![workspace.join("../SPBR-21.zip")];
    if let Some(home) = home_dir() {
        candidates.push(home.join("Downloads").join("SPBR-21.zip"));
    }

    env_existing(&["MTK_TEST_RESOURCE_PACK", "MTK_TEST_RESOURCEPACK"])
        .or_else(|| first_existing(candidates))
}

/// A Minecraft save directory containing `level.dat`.
pub fn save_world() -> Option<PathBuf> {
    env_existing(&["MTK_TEST_SAVE"])
}

/// A pre-built `models.bin` asset cache for serialization benchmarks.
pub fn models_cache_bin() -> Option<PathBuf> {
    env_existing(&["MTK_TEST_MODELS_CACHE"])
}

/// Resolves a path relative to the active [`assets_root`].
pub fn asset_path(relative: &str) -> Option<PathBuf> {
    let path = assets_root()?.join(relative);
    path.exists().then_some(path)
}

/// Reads a UTF-8 file relative to the active [`assets_root`].
pub fn read_asset(relative: &str) -> Option<String> {
    std::fs::read_to_string(asset_path(relative)?).ok()
}

/// Reads a UTF-8 file relative to the vendored [`fixtures_root`].
pub fn read_fixture(relative: &str) -> Option<String> {
    std::fs::read_to_string(fixtures_root().join(relative)).ok()
}

fn strip_namespace(id: &str) -> &str {
    id.split_once(':').map(|(_, rest)| rest).unwrap_or(id)
}

/// Maps a model id such as `minecraft:block/cube_all` to
/// `<root>/assets/minecraft/models/block/cube_all.json`.
pub fn model_json_path(root: &Path, model_id: &str) -> PathBuf {
    let name = strip_namespace(model_id);
    root.join("assets")
        .join("minecraft")
        .join("models")
        .join(format!("{name}.json"))
}

/// Maps a block id such as `minecraft:oak_stairs[facing=north]` to
/// `<root>/assets/minecraft/blockstates/oak_stairs.json`.
pub fn blockstate_json_path(root: &Path, block_id: &str) -> PathBuf {
    let base = block_id.split('[').next().unwrap_or(block_id);
    let name = strip_namespace(base);
    root.join("assets")
        .join("minecraft")
        .join("blockstates")
        .join(format!("{name}.json"))
}

/// Returns the active asset root or prints a skip notice and returns early.
///
/// ```no_run
/// # use mtk_testkit::require_assets;
/// let root = require_assets!();
/// assert!(root.join("assets/minecraft").is_dir());
/// ```
#[macro_export]
macro_rules! require_assets {
    () => {
        match $crate::assets_root() {
            Some(root) => root,
            None => {
                eprintln!(
                    "[mtk-testkit] skipping test: no Minecraft assets found. \
                     Set MTK_TEST_ASSETS or place an unpacked jar at ../mc"
                );
                return;
            }
        }
    };
}

/// Returns `true` when tests requiring live external network access are explicitly enabled.
///
/// Opt-in via `MTK_TEST_ONLINE=1` or `MTK_ONLINE_TESTS=1`.
pub fn is_online_enabled() -> bool {
    std::env::var("MTK_TEST_ONLINE")
        .or_else(|_| std::env::var("MTK_ONLINE_TESTS"))
        .map(|val| {
            val == "1" || val.eq_ignore_ascii_case("true") || val.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

/// Skips the enclosing test function if live network tests are not enabled via `MTK_TEST_ONLINE=1`.
///
/// ```no_run
/// # use mtk_testkit::require_online;
/// require_online!();
/// ```
#[macro_export]
macro_rules! require_online {
    () => {
        if !$crate::is_online_enabled() {
            eprintln!(
                "[mtk-testkit] skipping online test: set MTK_TEST_ONLINE=1 to enable live network tests"
            );
            return;
        }
    };
}
