use mtk_resource::{AtlasSource, ResourceLocation, ResourcePackStack};

use crate::{bake_paletted_permutation, DecodedSprite, RgbaBuffer};

/// Decodes and bakes PalettedPermutations sources into decoded sprites.
pub(crate) fn process_paletted_permutations(
    sources: &[AtlasSource],
    stack: &ResourcePackStack,
    decoded: &mut Vec<DecodedSprite>,
) {
    for source in sources {
        if let AtlasSource::PalettedPermutations {
            palette_key,
            permutations,
            textures,
        } = source
        {
            let key_bytes = match stack.open_texture_raw(palette_key) {
                Some(b) => b,
                None => continue,
            };
            let key_img = match RgbaBuffer::from_png_bytes(&key_bytes) {
                Ok(img) => img,
                Err(_) => continue,
            };

            for (perm_name, perm_loc) in permutations {
                let perm_bytes = match stack.open_texture_raw(perm_loc) {
                    Some(b) => b,
                    None => continue,
                };
                let perm_img = match RgbaBuffer::from_png_bytes(&perm_bytes) {
                    Ok(img) => img,
                    Err(_) => continue,
                };

                for tex_loc in textures {
                    let companions = stack.resolve_pbr_companions(tex_loc);
                    if let Some(bytes) = companions.albedo {
                        if let Ok(base_buf) = RgbaBuffer::from_png_bytes(&bytes) {
                            if let Ok(baked_perm) =
                                bake_paletted_permutation(&base_buf, &key_img, &perm_img)
                            {
                                let sprite_path = format!("{}_{}", tex_loc.path, perm_name);
                                let sprite_id =
                                    ResourceLocation::new(&tex_loc.namespace, sprite_path);
                                let fw = baked_perm.width;
                                let fh = baked_perm.height;
                                let normal = companions
                                    .normal
                                    .and_then(|b| RgbaBuffer::from_png_bytes(&b).ok())
                                    .map(|n| n.align_companion_to_albedo(fw, fh, 1));
                                let specular = companions
                                    .specular
                                    .and_then(|b| RgbaBuffer::from_png_bytes(&b).ok())
                                    .map(|s| s.align_companion_to_albedo(fw, fh, 1));
                                let overlay = companions
                                    .overlay
                                    .and_then(|b| RgbaBuffer::from_png_bytes(&b).ok())
                                    .map(|o| o.align_companion_to_albedo(fw, fh, 1));
                                decoded.push(DecodedSprite {
                                    sprite_id,
                                    albedo: baked_perm,
                                    normal,
                                    specular,
                                    overlay,
                                    emissive: None,
                                    normal_gl: None,
                                    orm: None,
                                    aux: None,
                                    frame_width: fw,
                                    frame_height: fh,
                                    frame_count: 1,
                                    metadata: None,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
}
