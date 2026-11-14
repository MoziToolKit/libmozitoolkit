#![forbid(unsafe_code)]
//! Procedural 2D extruded item model generator with 2D/1D greedy meshing.
//!
//! Mirrors `ItemModelGenerator` from vanilla Minecraft (1.21+ `builtin/generated` and
//! `minecraft:item/generated`), but applies:
//! 1. 2D Greedy Meshing to merge coplanar front (South) and back (North) quads;
//! 2. 1D Greedy Span Merging to merge adjacent perimeter side quads (Up, Down, Left, Right);
//! 3. Edge UV shrink protection to eliminate color bleeding across alpha boundaries;
//! 4. Centered coordinate layout around `(0, 0, 0)` with canonical $1/16$ block thickness.

use std::collections::HashMap;

use glam::{Vec2, Vec3};
use mtk_core::{greedy_mesh_2d_mask, greedy_span_1d, Direction, GreedyRect};

use super::options::ItemMeshOptions;
use crate::baker::baked_model::element::{BakedElement, BakedFace};
use crate::baker::BakedModel;
use crate::error::ModelError;

/// Bakes a 2D extruded item model from a flat RGBA8 pixel buffer.
///
/// * `pixels`: Row-major RGBA8 contiguous bytes (`length >= width * height * 4`).
/// * `width`: Image width in pixels.
/// * `height`: Image height in pixels.
/// * `texture`: Canonical texture identifier (e.g. `"minecraft:item/diamond_sword"`).
/// * `options`: Baking options controlling thickness, greedy merging, and UV shrink.
pub fn bake_item_generated_model(
    pixels: &[u8],
    width: u32,
    height: u32,
    texture: &str,
    options: &ItemMeshOptions,
) -> Result<BakedModel, ModelError> {
    let expected_len = (width as usize) * (height as usize) * 4;
    if pixels.len() < expected_len {
        return Err(ModelError::ItemBakeError(format!(
            "Pixel buffer too short: expected {} bytes for {}x{}, got {}",
            expected_len,
            width,
            height,
            pixels.len()
        )));
    }

    if width == 0 || height == 0 {
        return Ok(BakedModel {
            block_state: texture.to_string(),
            ..Default::default()
        });
    }

    let w_usize = width as usize;
    let h_usize = height as usize;

    // 1. Extract opacity mask based on alpha threshold
    let mut solid_mask = vec![false; w_usize * h_usize];
    let mut solid_count = 0;
    for y in 0..h_usize {
        for x in 0..w_usize {
            let a = pixels[(y * w_usize + x) * 4 + 3];
            if a >= options.alpha_threshold {
                solid_mask[y * w_usize + x] = true;
                solid_count += 1;
            }
        }
    }

    if solid_count == 0 {
        return Ok(BakedModel {
            block_state: texture.to_string(),
            ..Default::default()
        });
    }

    let half_depth = options.depth * 0.5;
    let dx = 1.0 / (width as f32);
    let dy = 1.0 / (height as f32);

    let mut elements = Vec::new();

    // 2. Generate front (South, +Z) and back (North, -Z) faces
    let front_back_rects = if options.use_greedy_meshing {
        greedy_mesh_2d_mask(&solid_mask, width, height)
    } else {
        // Fallback: 1 quad per solid pixel
        let mut unmerged = Vec::with_capacity(solid_count);
        for y in 0..h_usize {
            for x in 0..w_usize {
                if solid_mask[y * w_usize + x] {
                    unmerged.push(GreedyRect::new(x as u32, y as u32, 1, 1));
                }
            }
        }
        unmerged
    };

    for rect in front_back_rects {
        let x0 = -0.5 + (rect.x as f32) * dx;
        let x1 = -0.5 + ((rect.x + rect.w) as f32) * dx;
        let y0 = 0.5 - ((rect.y + rect.h) as f32) * dy;
        let y1 = 0.5 - (rect.y as f32) * dy;

        let u0 = (rect.x as f32) / (width as f32);
        let u1 = ((rect.x + rect.w) as f32) / (width as f32);
        let v0 = (rect.y as f32) / (height as f32);
        let v1 = ((rect.y + rect.h) as f32) / (height as f32);

        // --- South Face (Front, +Z) ---
        let south_face = BakedFace {
            direction: Direction::South,
            texture: texture.to_string(),
            uv_rot: 0.0,
            uv_bounds: [u0 * 16.0, v0 * 16.0, u1 * 16.0, v1 * 16.0],
            tint_index: -1,
            cullface: None,
            vertices: [
                Vec3::new(x0, y1, half_depth), // Top-Left
                Vec3::new(x0, y0, half_depth), // Bottom-Left
                Vec3::new(x1, y0, half_depth), // Bottom-Right
                Vec3::new(x1, y1, half_depth), // Top-Right
            ],
            uvs: [
                Vec2::new(u0, v0),
                Vec2::new(u0, v1),
                Vec2::new(u1, v1),
                Vec2::new(u1, v0),
            ],
            normal: Vec3::Z,
            atlas_uvs: None,
            atlas_chunk_id: None,
            atlas_texture_id: None,
        };

        let mut south_faces = HashMap::with_capacity(1);
        south_faces.insert(Direction::South, south_face);
        elements.push(BakedElement {
            from_pos: [x0, y0, half_depth],
            to_pos: [x1, y1, half_depth],
            faces: south_faces,
        });

        // --- North Face (Back, -Z, horizontally flipped UVs for canonical back texture alignment) ---
        let north_face = BakedFace {
            direction: Direction::North,
            texture: texture.to_string(),
            uv_rot: 0.0,
            uv_bounds: [u0 * 16.0, v0 * 16.0, u1 * 16.0, v1 * 16.0],
            tint_index: -1,
            cullface: None,
            vertices: [
                Vec3::new(x1, y1, -half_depth), // Top-Left from back view
                Vec3::new(x1, y0, -half_depth), // Bottom-Left from back view
                Vec3::new(x0, y0, -half_depth), // Bottom-Right from back view
                Vec3::new(x0, y1, -half_depth), // Top-Right from back view
            ],
            uvs: [
                Vec2::new(u1, v0),
                Vec2::new(u1, v1),
                Vec2::new(u0, v1),
                Vec2::new(u0, v0),
            ],
            normal: Vec3::NEG_Z,
            atlas_uvs: None,
            atlas_chunk_id: None,
            atlas_texture_id: None,
        };

        let mut north_faces = HashMap::with_capacity(1);
        north_faces.insert(Direction::North, north_face);
        elements.push(BakedElement {
            from_pos: [x0, y0, -half_depth],
            to_pos: [x1, y1, -half_depth],
            faces: north_faces,
        });
    }

    // 3. Generate extruded perimeter side quads (Up, Down, West, East)
    let shrink_u = options.uv_shrink / (width as f32);
    let shrink_v = options.uv_shrink / (height as f32);

    // --- UP Sides (+Y, facing upwards) ---
    // A pixel at (x, y) has an UP edge if y == 0 || !solid_mask[(y - 1) * w + x]
    for y in 0..h_usize {
        let mut row_up_mask = vec![false; w_usize];
        for x in 0..w_usize {
            if solid_mask[y * w_usize + x] && (y == 0 || !solid_mask[(y - 1) * w_usize + x]) {
                row_up_mask[x] = true;
            }
        }

        let spans = if options.use_greedy_meshing {
            greedy_span_1d(&row_up_mask)
        } else {
            row_up_mask
                .iter()
                .enumerate()
                .filter(|(_, &v)| v)
                .map(|(x, _)| (x as u32, 1))
                .collect()
        };

        let y_top = 0.5 - (y as f32) * dy;
        let v_center0 = (y as f32) / (height as f32) + shrink_v;
        let v_center1 = ((y + 1) as f32) / (height as f32) - shrink_v;

        for (x_start, span_w) in spans {
            let x0 = -0.5 + (x_start as f32) * dx;
            let x1 = -0.5 + ((x_start + span_w) as f32) * dx;
            let u0 = (x_start as f32) / (width as f32) + shrink_u;
            let u1 = ((x_start + span_w) as f32) / (width as f32) - shrink_u;

            let face = BakedFace {
                direction: Direction::Up,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds: [u0 * 16.0, v_center0 * 16.0, u1 * 16.0, v_center1 * 16.0],
                tint_index: -1,
                cullface: None,
                vertices: [
                    Vec3::new(x0, y_top, -half_depth),
                    Vec3::new(x0, y_top, half_depth),
                    Vec3::new(x1, y_top, half_depth),
                    Vec3::new(x1, y_top, -half_depth),
                ],
                uvs: [
                    Vec2::new(u0, v_center0),
                    Vec2::new(u0, v_center1),
                    Vec2::new(u1, v_center1),
                    Vec2::new(u1, v_center0),
                ],
                normal: Vec3::Y,
                atlas_uvs: None,
                atlas_chunk_id: None,
                atlas_texture_id: None,
            };

            let mut faces = HashMap::with_capacity(1);
            faces.insert(Direction::Up, face);
            elements.push(BakedElement {
                from_pos: [x0, y_top, -half_depth],
                to_pos: [x1, y_top, half_depth],
                faces,
            });
        }
    }

    // --- DOWN Sides (-Y, facing downwards) ---
    // A pixel at (x, y) has a DOWN edge if y == h - 1 || !solid_mask[(y + 1) * w + x]
    for y in 0..h_usize {
        let mut row_down_mask = vec![false; w_usize];
        for x in 0..w_usize {
            if solid_mask[y * w_usize + x]
                && (y + 1 == h_usize || !solid_mask[(y + 1) * w_usize + x])
            {
                row_down_mask[x] = true;
            }
        }

        let spans = if options.use_greedy_meshing {
            greedy_span_1d(&row_down_mask)
        } else {
            row_down_mask
                .iter()
                .enumerate()
                .filter(|(_, &v)| v)
                .map(|(x, _)| (x as u32, 1))
                .collect()
        };

        let y_bottom = 0.5 - ((y + 1) as f32) * dy;
        let v_center0 = (y as f32) / (height as f32) + shrink_v;
        let v_center1 = ((y + 1) as f32) / (height as f32) - shrink_v;

        for (x_start, span_w) in spans {
            let x0 = -0.5 + (x_start as f32) * dx;
            let x1 = -0.5 + ((x_start + span_w) as f32) * dx;
            let u0 = (x_start as f32) / (width as f32) + shrink_u;
            let u1 = ((x_start + span_w) as f32) / (width as f32) - shrink_u;

            let face = BakedFace {
                direction: Direction::Down,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds: [u0 * 16.0, v_center0 * 16.0, u1 * 16.0, v_center1 * 16.0],
                tint_index: -1,
                cullface: None,
                vertices: [
                    Vec3::new(x0, y_bottom, half_depth),
                    Vec3::new(x0, y_bottom, -half_depth),
                    Vec3::new(x1, y_bottom, -half_depth),
                    Vec3::new(x1, y_bottom, half_depth),
                ],
                uvs: [
                    Vec2::new(u0, v_center1),
                    Vec2::new(u0, v_center0),
                    Vec2::new(u1, v_center0),
                    Vec2::new(u1, v_center1),
                ],
                normal: Vec3::NEG_Y,
                atlas_uvs: None,
                atlas_chunk_id: None,
                atlas_texture_id: None,
            };

            let mut faces = HashMap::with_capacity(1);
            faces.insert(Direction::Down, face);
            elements.push(BakedElement {
                from_pos: [x0, y_bottom, -half_depth],
                to_pos: [x1, y_bottom, half_depth],
                faces,
            });
        }
    }

    // --- WEST Sides (-X, facing left) ---
    // A pixel at (x, y) has a WEST edge if x == 0 || !solid_mask[y * w + (x - 1)]
    for x in 0..w_usize {
        let mut col_west_mask = vec![false; h_usize];
        for y in 0..h_usize {
            if solid_mask[y * w_usize + x] && (x == 0 || !solid_mask[y * w_usize + (x - 1)]) {
                col_west_mask[y] = true;
            }
        }

        let spans = if options.use_greedy_meshing {
            greedy_span_1d(&col_west_mask)
        } else {
            col_west_mask
                .iter()
                .enumerate()
                .filter(|(_, &v)| v)
                .map(|(y, _)| (y as u32, 1))
                .collect()
        };

        let x_left = -0.5 + (x as f32) * dx;
        let u_center0 = (x as f32) / (width as f32) + shrink_u;
        let u_center1 = ((x + 1) as f32) / (width as f32) - shrink_u;

        for (y_start, span_h) in spans {
            let y_bot = 0.5 - ((y_start + span_h) as f32) * dy;
            let y_top = 0.5 - (y_start as f32) * dy;
            let v0 = (y_start as f32) / (height as f32) + shrink_v;
            let v1 = ((y_start + span_h) as f32) / (height as f32) - shrink_v;

            let face = BakedFace {
                direction: Direction::West,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds: [u_center0 * 16.0, v0 * 16.0, u_center1 * 16.0, v1 * 16.0],
                tint_index: -1,
                cullface: None,
                vertices: [
                    Vec3::new(x_left, y_top, -half_depth),
                    Vec3::new(x_left, y_bot, -half_depth),
                    Vec3::new(x_left, y_bot, half_depth),
                    Vec3::new(x_left, y_top, half_depth),
                ],
                uvs: [
                    Vec2::new(u_center0, v0),
                    Vec2::new(u_center0, v1),
                    Vec2::new(u_center1, v1),
                    Vec2::new(u_center1, v0),
                ],
                normal: Vec3::NEG_X,
                atlas_uvs: None,
                atlas_chunk_id: None,
                atlas_texture_id: None,
            };

            let mut faces = HashMap::with_capacity(1);
            faces.insert(Direction::West, face);
            elements.push(BakedElement {
                from_pos: [x_left, y_bot, -half_depth],
                to_pos: [x_left, y_top, half_depth],
                faces,
            });
        }
    }

    // --- EAST Sides (+X, facing right) ---
    // A pixel at (x, y) has an EAST edge if x + 1 == w || !solid_mask[y * w + (x + 1)]
    for x in 0..w_usize {
        let mut col_east_mask = vec![false; h_usize];
        for y in 0..h_usize {
            if solid_mask[y * w_usize + x]
                && (x + 1 == w_usize || !solid_mask[y * w_usize + (x + 1)])
            {
                col_east_mask[y] = true;
            }
        }

        let spans = if options.use_greedy_meshing {
            greedy_span_1d(&col_east_mask)
        } else {
            col_east_mask
                .iter()
                .enumerate()
                .filter(|(_, &v)| v)
                .map(|(y, _)| (y as u32, 1))
                .collect()
        };

        let x_right = -0.5 + ((x + 1) as f32) * dx;
        let u_center0 = (x as f32) / (width as f32) + shrink_u;
        let u_center1 = ((x + 1) as f32) / (width as f32) - shrink_u;

        for (y_start, span_h) in spans {
            let y_bot = 0.5 - ((y_start + span_h) as f32) * dy;
            let y_top = 0.5 - (y_start as f32) * dy;
            let v0 = (y_start as f32) / (height as f32) + shrink_v;
            let v1 = ((y_start + span_h) as f32) / (height as f32) - shrink_v;

            let face = BakedFace {
                direction: Direction::East,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds: [u_center0 * 16.0, v0 * 16.0, u_center1 * 16.0, v1 * 16.0],
                tint_index: -1,
                cullface: None,
                vertices: [
                    Vec3::new(x_right, y_top, half_depth),
                    Vec3::new(x_right, y_bot, half_depth),
                    Vec3::new(x_right, y_bot, -half_depth),
                    Vec3::new(x_right, y_top, -half_depth),
                ],
                uvs: [
                    Vec2::new(u_center1, v0),
                    Vec2::new(u_center1, v1),
                    Vec2::new(u_center0, v1),
                    Vec2::new(u_center0, v0),
                ],
                normal: Vec3::X,
                atlas_uvs: None,
                atlas_chunk_id: None,
                atlas_texture_id: None,
            };

            let mut faces = HashMap::with_capacity(1);
            faces.insert(Direction::East, face);
            elements.push(BakedElement {
                from_pos: [x_right, y_bot, -half_depth],
                to_pos: [x_right, y_top, half_depth],
                faces,
            });
        }
    }

    Ok(BakedModel {
        block_state: texture.to_string(),
        elements,
        is_cube: false,
        is_opaque: false,
        is_emissive: false,
        ..Default::default()
    })
}

/// Decodes a PNG byte slice and bakes an extruded item model.
pub fn bake_item_from_png_bytes(
    png_bytes: &[u8],
    texture: &str,
    options: &ItemMeshOptions,
) -> Result<BakedModel, ModelError> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png)
        .map_err(|e| ModelError::ItemBakeError(format!("Failed to decode PNG bytes: {}", e)))?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    bake_item_generated_model(&rgba, width, height, texture, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bake_empty_or_transparent_image() {
        let pixels = vec![0u8; 16 * 16 * 4];
        let model = bake_item_generated_model(
            &pixels,
            16,
            16,
            "minecraft:item/empty",
            &ItemMeshOptions::default(),
        )
        .unwrap();
        assert_eq!(model.elements.len(), 0);
    }

    #[test]
    fn test_bake_single_pixel_creates_closed_box() {
        let mut pixels = vec![0u8; 4 * 4 * 4];
        // 1 solid pixel at (x=1, y=1) in 4x4
        let (px, py) = (1usize, 1usize);
        let idx = (py * 4 + px) * 4;
        pixels[idx] = 255;
        pixels[idx + 1] = 0;
        pixels[idx + 2] = 0;
        pixels[idx + 3] = 255;

        let model = bake_item_generated_model(
            &pixels,
            4,
            4,
            "minecraft:item/test",
            &ItemMeshOptions::default(),
        )
        .unwrap();

        // 6 faces: South, North, Up, Down, West, East
        assert_eq!(model.elements.len(), 6);

        let dirs: Vec<Direction> = model
            .elements
            .iter()
            .flat_map(|el| el.faces.keys())
            .copied()
            .collect();
        assert!(dirs.contains(&Direction::South));
        assert!(dirs.contains(&Direction::North));
        assert!(dirs.contains(&Direction::Up));
        assert!(dirs.contains(&Direction::Down));
        assert!(dirs.contains(&Direction::West));
        assert!(dirs.contains(&Direction::East));
    }

    #[test]
    fn test_greedy_meshing_merges_solid_block() {
        // 4x4 solid block
        let pixels = vec![255u8; 4 * 4 * 4];
        let options_greedy = ItemMeshOptions {
            use_greedy_meshing: true,
            ..Default::default()
        };
        let model_greedy =
            bake_item_generated_model(&pixels, 4, 4, "minecraft:item/block", &options_greedy)
                .unwrap();

        // With greedy meshing, the 4x4 block is:
        // 1 front face, 1 back face, 1 up face, 1 down face, 1 west face, 1 east face = 6 elements!
        assert_eq!(model_greedy.elements.len(), 6);

        // Without greedy meshing:
        // 16 front faces + 16 back faces + 4 up + 4 down + 4 west + 4 east = 48 elements!
        let options_no_greedy = ItemMeshOptions {
            use_greedy_meshing: false,
            ..Default::default()
        };
        let model_no_greedy =
            bake_item_generated_model(&pixels, 4, 4, "minecraft:item/block", &options_no_greedy)
                .unwrap();
        assert_eq!(model_no_greedy.elements.len(), 48);

        // Reduction ratio: 6 / 48 = 87.5% reduction!
    }
}
