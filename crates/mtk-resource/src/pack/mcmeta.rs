//! Root `pack.mcmeta` metadata that changes how a pack is read.
//!
//! Only the sections that affect *reading* live here: `overlays` (per-version directories that
//! shadow the pack root) and, added separately, `filter` (resource-hiding patterns). The `pack`
//! section's display fields are irrelevant to us, and `formats` compatibility is a UI concern in
//! Minecraft, not a reading one.

use serde::Deserialize;

/// Client resource-pack format this crate targets.
///
/// Overlay applicability is tested against the *running game's* client format
/// (`Pack.readPackMetadata` -> `SharedConstants.getCurrentVersion().packVersion(CLIENT_RESOURCES)`),
/// not the pack's declared format. Value from `Minecraft_Source/version.json`:
/// `pack_version.resource_major` for 26.2.
pub const CURRENT_CLIENT_RESOURCE_FORMAT: u32 = 88;

/// The parts of `pack.mcmeta` we consume.
#[derive(Debug, Default, Deserialize)]
pub struct PackMetaFile {
    #[serde(default)]
    pub overlays: Option<OverlayMetadata>,
}

/// `pack.mcmeta` -> `overlays`.
#[derive(Debug, Default, Deserialize)]
pub struct OverlayMetadata {
    #[serde(default)]
    pub entries: Vec<OverlayEntryFile>,
}

/// One overlay declaration: a directory plus the pack formats it applies to.
#[derive(Debug, Default, Deserialize)]
pub struct OverlayEntryFile {
    #[serde(default)]
    pub min_format: Option<VersionValue>,
    #[serde(default)]
    pub max_format: Option<VersionValue>,
    #[serde(default)]
    pub formats: Option<RangeValue>,
    #[serde(default)]
    pub directory: String,
}

/// A pack format, written either as `88` or as `[88, 0]`.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum VersionValue {
    Major(u32),
    Pair(u32, u32),
}

impl VersionValue {
    /// `(major, minor)`; a bare int is `(major, 0)`, like `PackFormat.of(int)`.
    fn as_pair(self) -> (u32, u32) {
        match self {
            VersionValue::Major(major) => (major, 0),
            VersionValue::Pair(major, minor) => (major, minor),
        }
    }
}

/// An inclusive range of pack formats, in any shape Mojang's codec accepts.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(untagged)]
pub enum RangeValue {
    Single(u32),
    Pair(u32, u32),
    Bounds {
        min_inclusive: u32,
        max_inclusive: u32,
    },
}

impl RangeValue {
    fn bounds(self) -> (u32, u32) {
        match self {
            RangeValue::Single(value) => (value, value),
            RangeValue::Pair(min, max) => (min, max),
            RangeValue::Bounds {
                min_inclusive,
                max_inclusive,
            } => (min_inclusive, max_inclusive),
        }
    }
}

impl OverlayEntryFile {
    /// The `(major, minor)` format range this entry applies to, or `None` when it declares none.
    ///
    /// `min_format`/`max_format` (the post-64 form) win over the legacy `formats` range, matching
    /// `PackFormat.IntermediaryFormat.validate`. `formats` is a range of *major* versions
    /// (`InclusiveRange.codec(Codec.INT)`), so a legacy upper bound covers that whole major.
    fn applicable_range(&self) -> Option<((u32, u32), (u32, u32))> {
        match (self.min_format, self.max_format) {
            (Some(min), Some(max)) => Some((min.as_pair(), max.as_pair())),
            (None, None) => self
                .formats
                .map(|range| range.bounds())
                .map(|(min, max)| ((min, 0), (max, u32::MAX))),
            _ => None,
        }
    }

    /// Whether this overlay applies to `current_major` (the running client's resource format).
    pub fn is_applicable(&self, current_major: u32) -> bool {
        let current = (current_major, 0);
        self.applicable_range()
            .is_some_and(|(min, max)| min <= current && current <= max)
    }
}

impl PackMetaFile {
    /// Parses a `pack.mcmeta` body. Unknown sections are ignored, as in Minecraft.
    pub fn parse_json(bytes: &[u8]) -> Option<Self> {
        serde_json::from_slice(bytes).ok()
    }

    /// Applicable overlay directories, in declaration order.
    ///
    /// [`crate::pack::source::DirectoryPack`] / `ZipPack` give later entries the higher priority
    /// (`CompositePackResources` stacks `reverse(overlays)` in front of the pack root).
    pub fn applicable_overlays(&self, current: u32) -> Vec<String> {
        self.overlays
            .as_ref()
            .map(|overlays| {
                overlays
                    .entries
                    .iter()
                    .filter(|entry| entry.is_applicable(current))
                    .map(|entry| entry.directory.clone())
                    .filter(|directory| is_valid_overlay_dir(directory))
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// Minecraft validates the directory name as `[-_a-zA-Z0-9.]+` so an overlay can never escape the
/// pack root.
fn is_valid_overlay_dir(directory: &str) -> bool {
    !directory.is_empty()
        && directory
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
