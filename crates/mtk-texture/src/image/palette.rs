use crate::error::TextureError;
use crate::image::buffer::RgbaBuffer;
use std::collections::HashMap;

/// Extract ordered RGBA color palette from a 1xN or Nx1 palette texture.
fn extract_palette_colors(palette_img: &RgbaBuffer) -> Vec<[u8; 4]> {
    let mut colors = Vec::new();
    let w = palette_img.width;
    let h = palette_img.height;

    if w >= h {
        for x in 0..w {
            for y in 0..h {
                colors.push(palette_img.get_pixel(x, y));
            }
        }
    } else {
        for y in 0..h {
            for x in 0..w {
                colors.push(palette_img.get_pixel(x, y));
            }
        }
    }
    colors
}

/// Apply a single paletted color permutation to a base texture in-memory.
/// Non-zero alpha pixels of `base_image` matching `key_palette` are replaced with corresponding colors from `perm_palette`.
pub(crate) fn bake_paletted_permutation(
    base_image: &RgbaBuffer,
    key_palette: &RgbaBuffer,
    perm_palette: &RgbaBuffer,
) -> Result<RgbaBuffer, TextureError> {
    let key_colors = extract_palette_colors(key_palette);
    let perm_colors = extract_palette_colors(perm_palette);

    if key_colors.is_empty() || perm_colors.is_empty() {
        return Ok(base_image.clone());
    }

    // Build color lookup map: [r, g, b] -> [pr, pg, pb, pa]
    let mut color_map: HashMap<[u8; 3], [u8; 4]> = HashMap::new();
    let count = key_colors.len().min(perm_colors.len());
    for i in 0..count {
        let k = key_colors[i];
        let p = perm_colors[i];
        color_map.insert([k[0], k[1], k[2]], p);
    }

    let mut result = RgbaBuffer::new(base_image.width, base_image.height);

    for y in 0..base_image.height {
        for x in 0..base_image.width {
            let [r, g, b, a] = base_image.get_pixel(x, y);
            if a == 0 {
                continue;
            }

            if let Some(&[pr, pg, pb, pa]) = color_map.get(&[r, g, b]) {
                let final_a = (((a as u32) * (pa as u32)) / 255) as u8;
                result.set_pixel(x, y, [pr, pg, pb, final_a]);
            } else {
                result.set_pixel(x, y, [r, g, b, a]);
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paletted_permutation_baking() {
        let mut key_pal = RgbaBuffer::new(2, 1);
        key_pal.set_pixel(0, 0, [255, 0, 0, 255]);
        key_pal.set_pixel(1, 0, [0, 255, 0, 255]);

        let mut perm_pal = RgbaBuffer::new(2, 1);
        perm_pal.set_pixel(0, 0, [0, 0, 255, 255]);
        perm_pal.set_pixel(1, 0, [255, 255, 0, 255]);

        let mut base = RgbaBuffer::new(2, 2);
        base.set_pixel(0, 0, [255, 0, 0, 255]);
        base.set_pixel(1, 0, [0, 255, 0, 255]);
        base.set_pixel(0, 1, [255, 255, 255, 255]);
        base.set_pixel(1, 1, [0, 0, 0, 0]);

        let baked = bake_paletted_permutation(&base, &key_pal, &perm_pal).unwrap();
        assert_eq!(baked.get_pixel(0, 0), [0, 0, 255, 255]);
        assert_eq!(baked.get_pixel(1, 0), [255, 255, 0, 255]);
        assert_eq!(baked.get_pixel(0, 1), [255, 255, 255, 255]);
        assert_eq!(baked.get_pixel(1, 1), [0, 0, 0, 0]);
    }
}
