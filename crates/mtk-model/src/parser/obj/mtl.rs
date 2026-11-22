//! Wavefront `.mtl` (Material Template Library) parser for Forge / NeoForge OBJ models.

use std::collections::HashMap;

/// Returns `true` when `tex` looks like a Minecraft texture resource location (or `#var`)
/// rather than a local developer filesystem path accidentally baked into an `.mtl`.
#[inline]
pub(crate) fn is_valid_mc_texture_ref(tex: &str) -> bool {
    let s = tex.trim();
    if s.is_empty() || s.contains('\\') || s.contains("..") || s.chars().any(char::is_whitespace) {
        return false;
    }
    let bytes = s.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        // Reject Windows drive letters like `P:/...` or `C:...` while allowing 2+-char namespaces.
        return false;
    }
    true
}

/// Single material definition parsed from a Wavefront `.mtl` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtlMaterial {
    pub name: String,
    pub map_kd: Option<String>,
    pub tint_index: i16,
}

impl Default for MtlMaterial {
    fn default() -> Self {
        Self {
            name: String::new(),
            map_kd: None,
            tint_index: -1,
        }
    }
}

/// Collection of materials loaded from a single `.mtl` file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedMtlLibrary {
    pub materials: HashMap<String, MtlMaterial>,
}

impl ParsedMtlLibrary {
    /// Parses a Wavefront `.mtl` text payload.
    pub fn parse_str(text: &str) -> Self {
        let mut materials = HashMap::new();
        let mut current: Option<MtlMaterial> = None;

        for raw_line in text.lines() {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let mut parts = trimmed.splitn(2, char::is_whitespace);
            let cmd = parts.next().unwrap_or("");
            let arg = parts.next().unwrap_or("").trim();

            if cmd.eq_ignore_ascii_case("newmtl") {
                if let Some(prev) = current.take() {
                    if !prev.name.is_empty() {
                        materials.insert(prev.name.clone(), prev);
                    }
                }
                current = Some(MtlMaterial {
                    name: arg.to_string(),
                    ..Default::default()
                });
            } else if cmd.eq_ignore_ascii_case("map_Kd") {
                if let Some(ref mut mat) = current {
                    let value = extract_map_kd_target(arg);
                    if !value.is_empty() {
                        mat.map_kd = Some(value.to_string());
                    }
                }
            } else if cmd.eq_ignore_ascii_case("forge_TintIndex") {
                if let Some(ref mut mat) = current {
                    if let Ok(tint) = arg.parse::<i16>() {
                        mat.tint_index = tint;
                    }
                }
            }
        }

        if let Some(prev) = current {
            if !prev.name.is_empty() {
                materials.insert(prev.name.clone(), prev);
            }
        }

        Self { materials }
    }

    /// Iterates over valid non-variable `map_Kd` texture references declared directly in this library.
    pub fn iter_direct_texture_refs(&self) -> impl Iterator<Item = &str> {
        self.materials.values().filter_map(|mat| {
            mat.map_kd
                .as_deref()
                .filter(|s| !s.starts_with('#') && is_valid_mc_texture_ref(s))
        })
    }
}

fn extract_map_kd_target(arg: &str) -> &str {
    // Standard MTL allows optional flags (e.g. `-clamp on`, `-s 1 1 1`) before the texture name.
    // If flags are present, the texture reference is the final whitespace-delimited token;
    // otherwise preserve the full trimmed string so spaces in broken local paths are rejected cleanly.
    if arg.starts_with('-') {
        arg.split_whitespace().last().unwrap_or("")
    } else {
        arg
    }
}
