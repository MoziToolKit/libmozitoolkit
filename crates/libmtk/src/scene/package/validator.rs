//! Scene interchange package validation engine.

use mtk_model::BakedModel;

use super::reader::ScenePackageReader;
use crate::MtkError;

/// Integrity and validation report produced by `ScenePackageReader::validate`.
#[derive(Debug, Clone, Default)]
pub struct ValidationReport {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

/// Performs an integrity check on models, atlas textures, colormaps, dynamic entities, and CTM rules.
pub fn validate_scene_package(pkg: &ScenePackageReader) -> Result<ValidationReport, MtkError> {
    let mut report = ValidationReport::default();
    let reader = pkg.inner_reader();

    // 1. Check colormaps PNG validity and decodability
    if pkg.has_colormaps() {
        match pkg.read_colormaps() {
            Ok(Some(_)) => {}
            Ok(None) => {}
            Err(e) => {
                report
                    .errors
                    .push(format!("Corrupted colormap in package: {e}"));
            }
        }
    }

    // 2. Check Biome Resolver
    if reader.has_chunk("biome/mapping") || reader.has_chunk("biome/resolver") {
        if let Err(e) = pkg.read_biome_resolver() {
            report
                .errors
                .push(format!("Invalid BiomeResolver in package: {e}"));
        }
    }

    // 3. Read and check Atlas Mapping
    let maybe_atlas_map = match pkg.read_atlas_mapping() {
        Ok(Some(map)) => Some(map),
        Ok(None) => {
            report
                .errors
                .push("Scene package is missing 'atlas/mapping' chunk".to_string());
            None
        }
        Err(e) => {
            report
                .errors
                .push(format!("Corrupted 'atlas/mapping' chunk: {e}"));
            None
        }
    };

    // 4. Verify that each atlas texture slice referenced by chunk metadata actually exists
    if let Some(ref map) = maybe_atlas_map {
        for chunk in &map.chunks {
            for slot in chunk.active_textures() {
                let chunk_path = format!("atlas/textures/{}", slot.file_name);
                if !reader.has_chunk(&chunk_path) {
                    report.errors.push(format!(
                        "Missing atlas texture slice '{chunk_path}' for chunk_id {}",
                        chunk.chunk_id
                    ));
                }
            }
        }
    }

    // 5. Read Models Database
    let maybe_models = match pkg.read_models() {
        Ok(Some(models)) => Some(models),
        Ok(None) => {
            report
                .errors
                .push("Scene package is missing 'models/database' chunk".to_string());
            None
        }
        Err(e) => {
            report
                .errors
                .push(format!("Corrupted 'models/database' chunk: {e}"));
            None
        }
    };

    // 6. Check Models (elements, faces, obj_faces, and variant groups) vs Atlas Mapping
    if let (Some(models), Some(map)) = (maybe_models.as_ref(), maybe_atlas_map.as_ref()) {
        let check_face = |face_tex: &str, source_desc: &str, rep: &mut ValidationReport| {
            if !face_tex.is_empty() && map.resolve_texture_key(face_tex).is_none() {
                rep.warnings.push(format!(
                    "{source_desc} references texture '{face_tex}' not found in mini-atlas"
                ));
            }
        };

        let check_model = |model: &BakedModel, desc: &str, rep: &mut ValidationReport| {
            for element in &model.elements {
                for face in element.faces.values() {
                    check_face(&face.texture, &format!("{desc} (element face)"), rep);
                }
            }
            for face in &model.faces {
                check_face(&face.texture, &format!("{desc} (directional face)"), rep);
            }
            for obj_face in &model.obj_faces {
                check_face(&obj_face.texture, &format!("{desc} (obj face)"), rep);
            }
        };

        for (state, model) in &models.models {
            check_model(model, &format!("Model for state '{state}'"), &mut report);
        }

        for (state, group) in &models.variant_groups {
            for (idx, model) in group.models.iter().enumerate() {
                check_model(
                    model,
                    &format!("Variant group '{state}' [variant {idx}]"),
                    &mut report,
                );
            }
        }
    }

    // 7. Check CTM Rules vs Mini-Atlas
    if pkg.has_ctm_rules() {
        match pkg.read_ctm_solver() {
            Ok(Some(solver)) => {
                if let Some(ref map) = maybe_atlas_map {
                    for rule in &solver.rules {
                        for tile in rule.tiles.iter().flatten() {
                            if map.lookup(tile).is_none() {
                                report.warnings.push(format!(
                                    "CTM rule '{}' references tile '{}' not found in mini-atlas",
                                    rule.name, tile
                                ));
                            }
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                report
                    .errors
                    .push(format!("Corrupted CTM rules JSON in package: {e}"));
            }
        }
    }

    // 8. Check Dynamic Entity Slots vs Mini-Atlas Sprites
    if pkg.has_dynamic_entity_slots() {
        match pkg.read_dynamic_entity_slots() {
            Ok(Some(slots)) => {
                if let Some(ref map) = maybe_atlas_map {
                    for (key, (_chunk_id, _slot_idx)) in &slots {
                        if map.resolve_texture_key(key).is_none() {
                            report.errors.push(format!(
                                "Dynamic entity slot '{key}' cannot be resolved to any sprite in mini-atlas"
                            ));
                        }
                    }
                }
            }
            Ok(None) => {}
            Err(e) => {
                report
                    .errors
                    .push(format!("Corrupted entity/slots in package: {e}"));
            }
        }
    }

    report.is_valid = report.errors.is_empty();
    Ok(report)
}
