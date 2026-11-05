use crate::image::buffer::RgbaBuffer;

/// Apply edge clamping padding around a sprite slot to prevent Mipmap / Bilinear texture bleeding.
pub(crate) fn apply_edge_clamping_padding(
    atlas_buffer: &mut RgbaBuffer,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    padding: u32,
) {
    if padding == 0 || w == 0 || h == 0 {
        return;
    }

    // Top and Bottom padding rows
    for p in 1..=padding {
        // Top edge replicated upward
        if y >= p {
            for col in 0..w {
                let px = atlas_buffer.get_pixel(x + col, y);
                atlas_buffer.set_pixel(x + col, y - p, px);
            }
        }
        // Bottom edge replicated downward
        if y + h + p - 1 < atlas_buffer.height {
            for col in 0..w {
                let px = atlas_buffer.get_pixel(x + col, y + h - 1);
                atlas_buffer.set_pixel(x + col, y + h + p - 1, px);
            }
        }
    }

    // Left and Right padding columns (including corners)
    for p in 1..=padding {
        let top_limit = y.saturating_sub(padding);
        let bottom_limit = (y + h + padding).min(atlas_buffer.height);

        for row in top_limit..bottom_limit {
            // Left edge replicated leftward
            if x >= p {
                let px = atlas_buffer.get_pixel(x, row);
                atlas_buffer.set_pixel(x - p, row, px);
            }
            // Right edge replicated rightward
            if x + w + p - 1 < atlas_buffer.width {
                let px = atlas_buffer.get_pixel(x + w - 1, row);
                atlas_buffer.set_pixel(x + w + p - 1, row, px);
            }
        }
    }
}
