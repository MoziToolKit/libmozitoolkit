//! # Custom Mesh Attribute Merging
//!
//! Handles dictionary merging, index remapping, and capacity pre-allocation
//! for custom attributes during batch mesh concatenation.

use std::collections::HashMap;

use super::MeshData;
use crate::attributes::{AttributeData, AttributeDomain, MeshAttribute};

/// Pre-seeds the merged attribute map with empty buffers sized for the final face count.
pub(crate) fn seed_custom_attributes(
    target: &mut HashMap<String, MeshAttribute>,
    meshes: &[&MeshData],
    total_faces: usize,
    total_verts: usize,
    total_corners: usize,
) {
    for &m in meshes {
        for (name, attr) in &m.custom_attributes {
            if target.contains_key(name) {
                continue;
            }
            let capacity = match attr.domain {
                AttributeDomain::Face => total_faces,
                AttributeDomain::Point => total_verts,
                AttributeDomain::Corner => total_corners,
                _ => attr.len(),
            };
            target.insert(
                name.clone(),
                MeshAttribute::new(
                    name.clone(),
                    attr.domain,
                    attr.data.empty_with_capacity(capacity),
                ),
            );
        }
    }
}

fn remap_attribute_indices(data: &AttributeData, remap: &[usize]) -> AttributeData {
    match data {
        AttributeData::Int32(src) => {
            let remapped: Vec<i32> = src
                .iter()
                .map(|&idx| {
                    if idx >= 0 && (idx as usize) < remap.len() {
                        remap[idx as usize] as i32
                    } else {
                        idx
                    }
                })
                .collect();
            AttributeData::Int32(remapped)
        }
        AttributeData::Int16(src) => {
            let remapped: Vec<i16> = src
                .iter()
                .map(|&idx| {
                    if idx >= 0 && (idx as usize) < remap.len() {
                        remap[idx as usize] as i16
                    } else {
                        idx
                    }
                })
                .collect();
            AttributeData::Int16(remapped)
        }
        AttributeData::UInt16(src) => {
            let remapped: Vec<u16> = src
                .iter()
                .map(|&idx| {
                    if (idx as usize) < remap.len() {
                        remap[idx as usize] as u16
                    } else {
                        idx
                    }
                })
                .collect();
            AttributeData::UInt16(remapped)
        }
        AttributeData::UInt32(src) => {
            let remapped: Vec<u32> = src
                .iter()
                .map(|&idx| {
                    if (idx as usize) < remap.len() {
                        remap[idx as usize] as u32
                    } else {
                        idx
                    }
                })
                .collect();
            AttributeData::UInt32(remapped)
        }
        _ => data.clone(),
    }
}

fn append_remapped_indices(dst: &mut AttributeData, src: &AttributeData, remap: &[usize]) {
    match (dst, src) {
        (AttributeData::Int32(dst), AttributeData::Int32(src)) => {
            dst.reserve(src.len());
            for &idx in src {
                if idx >= 0 && (idx as usize) < remap.len() {
                    dst.push(remap[idx as usize] as i32);
                } else {
                    dst.push(idx);
                }
            }
        }
        (AttributeData::Int16(dst), AttributeData::Int16(src)) => {
            dst.reserve(src.len());
            for &idx in src {
                if idx >= 0 && (idx as usize) < remap.len() {
                    dst.push(remap[idx as usize] as i16);
                } else {
                    dst.push(idx);
                }
            }
        }
        (AttributeData::UInt16(dst), AttributeData::UInt16(src)) => {
            dst.reserve(src.len());
            for &idx in src {
                if (idx as usize) < remap.len() {
                    dst.push(remap[idx as usize] as u16);
                } else {
                    dst.push(idx);
                }
            }
        }
        (AttributeData::UInt32(dst), AttributeData::UInt32(src)) => {
            dst.reserve(src.len());
            for &idx in src {
                if (idx as usize) < remap.len() {
                    dst.push(remap[idx as usize] as u32);
                } else {
                    dst.push(idx);
                }
            }
        }
        _ => {}
    }
}

fn merge_palette_attribute_pair(
    target: &mut HashMap<String, MeshAttribute>,
    other: &HashMap<String, MeshAttribute>,
    pal_name: &str,
    idx_name: &str,
) -> bool {
    let (Some(other_pal_attr), Some(other_idx_attr)) = (other.get(pal_name), other.get(idx_name))
    else {
        return false;
    };
    let AttributeData::String(other_pal) = &other_pal_attr.data else {
        return false;
    };

    if !target.contains_key(pal_name) && !target.contains_key(idx_name) {
        target.insert(pal_name.to_string(), other_pal_attr.clone());
        target.insert(idx_name.to_string(), other_idx_attr.clone());
        return true;
    }

    if !target.contains_key(pal_name) {
        target.insert(pal_name.to_string(), other_pal_attr.clone());
    }

    let Some(target_pal_attr) = target.get_mut(pal_name) else {
        return false;
    };
    let AttributeData::String(target_pal) = &mut target_pal_attr.data else {
        return false;
    };

    let mut remap = Vec::with_capacity(other_pal.len());
    for s in other_pal {
        if let Some(pos) = target_pal.iter().position(|x| x == s) {
            remap.push(pos);
        } else {
            let new_pos = target_pal.len();
            target_pal.push(s.clone());
            remap.push(new_pos);
        }
    }

    if !target.contains_key(idx_name) {
        let remapped_data = remap_attribute_indices(&other_idx_attr.data, &remap);
        target.insert(
            idx_name.to_string(),
            MeshAttribute::new(idx_name.to_string(), other_idx_attr.domain, remapped_data),
        );
        return true;
    }

    let Some(target_idx_attr) = target.get_mut(idx_name) else {
        return false;
    };

    append_remapped_indices(&mut target_idx_attr.data, &other_idx_attr.data, &remap);
    true
}

impl MeshData {
    /// Helper to merge custom attributes from `other` into `target`.
    pub(crate) fn extend_custom_attributes(
        target: &mut HashMap<String, MeshAttribute>,
        other: &HashMap<String, MeshAttribute>,
    ) {
        use crate::attributes::constants::*;

        const PALETTE_PAIRS: [(&str, &str); 4] = [
            (ATTR_SOURCE_TEXTURE_PALETTE, ATTR_SOURCE_TEXTURE_IDX),
            (ATTR_PALETTE, ATTR_STATE_IDX),
            (ATTR_BIOME_PALETTE, ATTR_BIOME_IDX),
            (ATTR_ENTITY_PALETTE, ATTR_ENTITY_IDX),
        ];

        let mut handled_keys = std::collections::HashSet::new();
        for &(pal_name, idx_name) in &PALETTE_PAIRS {
            if other.contains_key(pal_name)
                && other.contains_key(idx_name)
                && merge_palette_attribute_pair(target, other, pal_name, idx_name)
            {
                handled_keys.insert(pal_name);
                handled_keys.insert(idx_name);
            }
        }

        for (name, attr) in other {
            if handled_keys.contains(name.as_str()) {
                continue;
            }
            if let Some(existing) = target.get_mut(name) {
                if existing.domain == attr.domain {
                    match (&mut existing.data, &attr.data) {
                        (AttributeData::Float(a), AttributeData::Float(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::Float2(a), AttributeData::Float2(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::Float3(a), AttributeData::Float3(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::Float4(a), AttributeData::Float4(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::Int8(a), AttributeData::Int8(b)) => a.extend_from_slice(b),
                        (AttributeData::Int16(a), AttributeData::Int16(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::Int32(a), AttributeData::Int32(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::UInt8(a), AttributeData::UInt8(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::UInt16(a), AttributeData::UInt16(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::UInt32(a), AttributeData::UInt32(b)) => {
                            a.extend_from_slice(b)
                        }
                        (AttributeData::Bool(a), AttributeData::Bool(b)) => a.extend_from_slice(b),
                        (AttributeData::String(a), AttributeData::String(b)) => {
                            a.extend_from_slice(b)
                        }
                        _ => {}
                    }
                }
            } else {
                target.insert(name.clone(), attr.clone());
            }
        }
    }
}
