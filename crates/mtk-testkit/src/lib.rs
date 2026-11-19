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

//! 2. Conventional local paths adjacent to the workspace (`../mc`, `../Minecraft_Source`,
//!    `../26.2-Fabric.jar`, `../SPBR-21.zip`, `../mtk-save-world`, ...).
//! 3. A minimal set of vendored JSON fixtures shipped inside this crate (`fixtures/mc`).
//!
//! Tests that require the *full* vanilla asset set (e.g. canonical debug-world generation)
//! should query [`real_assets_root`] and skip when it returns `None`; hermetic tests can use
//! [`assets_root`], which always falls back to the vendored fixtures.
//!
//! A skip is never silent: every skipped test goes through [`skip`], which prints a greppable
//! `[mtk-testkit] SKIP ...` line. Set `MTK_REQUIRE_REAL_ASSETS=1` (CI does) to turn those skips
//! into failures, so a fixtures-only run cannot be mistaken for full-asset coverage.
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
//! | `MTK_REQUIRE_REAL_ASSETS` | Fail instead of skipping when real assets are missing |

use std::path::{Path, PathBuf};

/// Directory name of the vendored minimal fixtures shipped with this crate.
const FIXTURES_DIR: &str = "fixtures";

/// Conventional unpacked-client directories, relative to the workspace root.
///
/// `../mc` is the documented CI layout; `../Minecraft_Source` is the name used by the
/// developer workspace that ships alongside this repo, and `../Minecraft_Unpacked` is what
/// several unpack scripts produce. They are all the same shape: a root containing
/// `assets/minecraft`.
const ASSET_ROOT_CANDIDATES: &[&str] = &[
    "../mc",
    "mc",
    "../Minecraft_Source",
    "Minecraft_Source",
    "../Minecraft_Unpacked",
    "Minecraft_Unpacked",
];

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
    first_existing(
        ASSET_ROOT_CANDIDATES
            .iter()
            .map(|relative| workspace.join(relative)),
    )
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

/// `true` when missing assets must fail the suite instead of skipping it.
///
/// Set `MTK_REQUIRE_REAL_ASSETS=1` in CI and in the local pre-delivery gate.
pub fn real_assets_required() -> bool {
    std::env::var("MTK_REQUIRE_REAL_ASSETS")
        .map(|value| {
            value == "1" || value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("yes")
        })
        .unwrap_or(false)
}

/// Records a deliberate skip of a test that needs unavailable external assets.
///
/// Always prints a greppable `[mtk-testkit] SKIP ...` line so a green run cannot hide the fact
/// that the expensive coverage did not execute. When `MTK_REQUIRE_REAL_ASSETS` is set the skip
/// becomes a panic, which is how CI and the local gate refuse a fixtures-only run.
///
/// ```no_run
/// # use mtk_testkit::skip;
/// let Some(root) = mtk_testkit::real_assets_root() else {
///     skip("no full vanilla asset set");
///     return;
/// };
/// ```
pub fn skip(reason: &str) {
    if real_assets_required() {
        panic!(
            "[mtk-testkit] required assets are unavailable: {reason}. \
             Set MTK_TEST_ASSETS (or MTK_REQUIRE_REAL_ASSETS=0 to allow skipping)."
        );
    }
    eprintln!("[mtk-testkit] SKIP {reason}");
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
    if let Some(path) = env_existing(&["MTK_TEST_SAVE"]) {
        return Some(path);
    }

    let workspace = workspace_root();
    let mut candidates = vec![
        workspace.join("../mtk-save-world"),
        workspace.join("../saves/mtk-save-world"),
        workspace.join("../saves/test_world"),
    ];
    if let Some(home) = home_dir() {
        candidates.push(home.join("saves/test_world"));
    }
    first_existing(candidates).filter(|path| path.join("level.dat").exists())
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
                $crate::skip("no Minecraft assets found (vendored fixtures missing)");
                return;
            }
        }
    };
}

/// Returns the full vanilla asset root, or skips/fails the enclosing test.
///
/// Use this instead of `real_assets_root()` when the test asserts on vanilla completeness:
/// a fixtures-only run must be visible (and, with `MTK_REQUIRE_REAL_ASSETS=1`, fatal).
///
/// ```no_run
/// # use mtk_testkit::require_real_assets;
/// let root = require_real_assets!();
/// assert!(root.join("assets/minecraft/blockstates").is_dir());
/// ```
#[macro_export]
macro_rules! require_real_assets {
    () => {
        match $crate::real_assets_root() {
            Some(root) => root,
            None => {
                $crate::skip("no full vanilla asset set");
                return;
            }
        }
    };
}

/// Skips the enclosing test with an explicit reason (see [`skip`]).
#[macro_export]
macro_rules! skip_unless_required {
    ($reason:expr) => {
        $crate::skip($reason)
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
