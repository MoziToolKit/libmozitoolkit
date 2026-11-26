use crate::error::TextureError;
use std::cmp::Ordering;

/// Helper: find smallest power of two greater than or equal to `val`.
#[inline]
pub(crate) fn smallest_encompassing_power_of_two(val: u32) -> u32 {
    if val == 0 {
        return 1;
    }
    let mut v = val - 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

/// Align `input` up to a multiple of `1 << mip_level` (`Stitcher.smallestFittingMinTexel`).
#[inline]
fn smallest_fitting_min_texel(input: u32, mip_level: u32) -> u32 {
    let align = 1u32.checked_shl(mip_level).unwrap_or(u32::MAX);
    if align <= 1 || input.is_multiple_of(align) {
        input
    } else {
        ((input / align) + 1) * align
    }
}

/// An entry registered into the Stitcher before layout.
#[derive(Debug, Clone)]
pub(crate) struct StitcherHolder<T> {
    pub(crate) entry: T,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) name: String,
}

/// A placed sprite layout slot.
#[derive(Debug, Clone)]
pub(crate) struct StitchedSlot<T> {
    pub(crate) entry: T,
    pub(crate) x: u32,
    pub(crate) y: u32,
    #[cfg(test)]
    pub(crate) width: u32,
    #[cfg(test)]
    pub(crate) height: u32,
}

/// Result of stitching one or more atlas sheets (chunks).
#[derive(Debug, Clone)]
pub(crate) struct StitchedAtlas<T> {
    pub(crate) chunks: Vec<StitchedChunk<T>>,
}

#[derive(Debug, Clone)]
pub(crate) struct StitchedChunk<T> {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) slots: Vec<StitchedSlot<T>>,
}

#[derive(Debug, Clone, Copy)]
struct FreeRect {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Power-of-2 MaxRects-BSSF 2D Texture Stitcher.
///
/// Sorts holders following Minecraft's `HOLDER_COMPARATOR` (`Height desc -> Width desc -> Name asc`)
/// and packs them into unified maximal free rectangles across power-of-2 sheets (avoiding the
/// internal seam holes created by isolated power-of-2 expansion islands when sprites carry padding).
pub(crate) struct Stitcher<T> {
    max_width: u32,
    max_height: u32,
    mip_level: u32,
    holders: Vec<StitcherHolder<T>>,
    pinned_holder: Option<StitcherHolder<T>>,
}

impl<T: Clone> Stitcher<T> {
    /// Create a new Stitcher with maximum atlas dimensions and mipmap level.
    pub fn new(max_width: u32, max_height: u32, mip_level: u32) -> Self {
        Self {
            max_width,
            max_height,
            mip_level,
            holders: Vec::new(),
            pinned_holder: None,
        }
    }

    /// Register a pinned sprite entry guaranteed to be placed at chunk 0, slot 0 (x=0, y=0).
    pub fn register_pinned_sprite(
        &mut self,
        entry: T,
        width: u32,
        height: u32,
        name: impl Into<String>,
    ) {
        self.pinned_holder = Some(StitcherHolder {
            entry,
            width: smallest_fitting_min_texel(width, self.mip_level),
            height: smallest_fitting_min_texel(height, self.mip_level),
            name: name.into(),
        });
    }

    /// Register a sprite entry with its single-frame width, height, and canonical name.
    pub fn register_sprite(&mut self, entry: T, width: u32, height: u32, name: impl Into<String>) {
        self.holders.push(StitcherHolder {
            entry,
            width: smallest_fitting_min_texel(width, self.mip_level),
            height: smallest_fitting_min_texel(height, self.mip_level),
            name: name.into(),
        });
    }

    /// Execute the layout algorithm, automatically splitting into subsequent chunks if needed.
    pub fn stitch(&mut self) -> Result<StitchedAtlas<T>, TextureError> {
        if self.holders.is_empty() && self.pinned_holder.is_none() {
            return Ok(StitchedAtlas { chunks: Vec::new() });
        }

        // Sort holders strictly following Minecraft vanilla HOLDER_COMPARATOR:
        // Height desc -> Width desc -> Name asc
        self.holders.sort_by(|a, b| {
            let h_ord = b.height.cmp(&a.height);
            if h_ord != Ordering::Equal {
                return h_ord;
            }
            let w_ord = b.width.cmp(&a.width);
            if w_ord != Ordering::Equal {
                return w_ord;
            }
            a.name.cmp(&b.name)
        });

        let mut chunks = Vec::new();
        let mut unplaced = self.holders.clone();
        let mut pinned = self.pinned_holder.clone();

        while !unplaced.is_empty() || pinned.is_some() {
            let (chunk, remaining) = self.stitch_single_chunk(pinned.take(), unplaced)?;
            chunks.push(chunk);
            unplaced = remaining;
        }

        Ok(StitchedAtlas { chunks })
    }

    fn stitch_single_chunk(
        &self,
        pinned: Option<StitcherHolder<T>>,
        candidates: Vec<StitcherHolder<T>>,
    ) -> Result<(StitchedChunk<T>, Vec<StitcherHolder<T>>), TextureError> {
        let mut min_req_w = 1u32;
        let mut min_req_h = 1u32;
        let mut total_area = 0u64;

        for holder in pinned.iter().chain(candidates.iter()) {
            if holder.width > self.max_width || holder.height > self.max_height {
                return Err(TextureError::Stitcher(format!(
                    "Sprite '{}' dimensions ({}x{}) exceed maximum atlas size ({}x{})",
                    holder.name, holder.width, holder.height, self.max_width, self.max_height
                )));
            }
            min_req_w = min_req_w.max(holder.width);
            min_req_h = min_req_h.max(holder.height);
            total_area += holder.width as u64 * holder.height as u64;
        }

        let max_area = self.max_width as u64 * self.max_height as u64;
        if total_area <= max_area {
            let mut pow_ws = Vec::new();
            let mut w = smallest_encompassing_power_of_two(min_req_w);
            while w <= self.max_width {
                pow_ws.push(w);
                match w.checked_mul(2) {
                    Some(next) if next > w => w = next,
                    _ => break,
                }
            }
            let mut pow_hs = Vec::new();
            let mut h = smallest_encompassing_power_of_two(min_req_h);
            while h <= self.max_height {
                pow_hs.push(h);
                match h.checked_mul(2) {
                    Some(next) if next > h => h = next,
                    _ => break,
                }
            }

            let mut dims: Vec<(u64, u32, bool, u32, u32)> = Vec::new();
            for &cw in &pow_ws {
                for &ch in &pow_hs {
                    let area = cw as u64 * ch as u64;
                    if area >= total_area {
                        dims.push((area, cw.max(ch), cw < ch, cw, ch));
                    }
                }
            }
            dims.sort_unstable();

            for (_, _, _, cw, ch) in dims {
                if let Some((slots, unplaced)) =
                    pack_maxrects(cw, ch, pinned.as_ref(), &candidates, false)
                {
                    if unplaced.is_empty() {
                        return Ok((
                            StitchedChunk {
                                width: cw,
                                height: ch,
                                slots,
                            },
                            Vec::new(),
                        ));
                    }
                }
            }
        }

        // Spillover onto max-sized sheet (or multi-chunk split)
        let cw = smallest_encompassing_power_of_two(min_req_w).max(self.max_width);
        let ch = smallest_encompassing_power_of_two(min_req_h).max(self.max_height);
        let cw = cw.min(self.max_width);
        let ch = ch.min(self.max_height);

        let (slots, unplaced) = pack_maxrects(cw, ch, pinned.as_ref(), &candidates, true)
            .ok_or_else(|| {
                TextureError::Stitcher("Failed to place pinned sprite into max atlas".to_string())
            })?;

        if slots.is_empty() {
            return Err(TextureError::Stitcher(
                "No sprites could be placed in chunk".to_string(),
            ));
        }

        Ok((
            StitchedChunk {
                width: cw,
                height: ch,
                slots,
            },
            unplaced,
        ))
    }
}

fn pack_maxrects<T: Clone>(
    canvas_w: u32,
    canvas_h: u32,
    pinned: Option<&StitcherHolder<T>>,
    candidates: &[StitcherHolder<T>],
    allow_unplaced: bool,
) -> Option<(Vec<StitchedSlot<T>>, Vec<StitcherHolder<T>>)> {
    let mut free_rects = vec![FreeRect {
        x: 0,
        y: 0,
        w: canvas_w,
        h: canvas_h,
    }];
    let mut slots = Vec::with_capacity(candidates.len() + usize::from(pinned.is_some()));
    let mut unplaced = Vec::new();

    if let Some(pin) = pinned {
        if pin.width > canvas_w || pin.height > canvas_h {
            return None;
        }
        place_rect_and_split(&mut free_rects, 0, 0, pin.width, pin.height);
        slots.push(StitchedSlot {
            entry: pin.entry.clone(),
            x: 0,
            y: 0,
            #[cfg(test)]
            width: pin.width,
            #[cfg(test)]
            height: pin.height,
        });
    }

    for holder in candidates {
        let w = holder.width;
        let h = holder.height;

        let mut best: Option<((u32, u32, u32, u32), u32, u32)> = None;
        for rect in &free_rects {
            if w <= rect.w && h <= rect.h {
                let short_side = (rect.w - w).min(rect.h - h);
                let long_side = (rect.w - w).max(rect.h - h);
                let score = (short_side, long_side, rect.y, rect.x);
                if best.as_ref().is_none_or(|(b_score, _, _)| score < *b_score) {
                    best = Some((score, rect.x, rect.y));
                }
            }
        }

        if let Some((_, px, py)) = best {
            place_rect_and_split(&mut free_rects, px, py, w, h);
            slots.push(StitchedSlot {
                entry: holder.entry.clone(),
                x: px,
                y: py,
                #[cfg(test)]
                width: w,
                #[cfg(test)]
                height: h,
            });
        } else if allow_unplaced {
            unplaced.push(holder.clone());
        } else {
            return None;
        }
    }

    Some((slots, unplaced))
}

fn place_rect_and_split(free_rects: &mut Vec<FreeRect>, bx: u32, by: u32, bw: u32, bh: u32) {
    let bx2 = bx + bw;
    let by2 = by + bh;
    let orig_len = free_rects.len();
    let mut i = 0;

    while i < orig_len {
        let f = free_rects[i];
        let fx2 = f.x + f.w;
        let fy2 = f.y + f.h;
        if bx >= fx2 || bx2 <= f.x || by >= fy2 || by2 <= f.y {
            i += 1;
            continue;
        }
        // Remove intersecting rect by swapping with the tail of the original slice or push replacements
        if bx > f.x {
            free_rects.push(FreeRect {
                x: f.x,
                y: f.y,
                w: bx - f.x,
                h: f.h,
            });
        }
        if bx2 < fx2 {
            free_rects.push(FreeRect {
                x: bx2,
                y: f.y,
                w: fx2 - bx2,
                h: f.h,
            });
        }
        if by > f.y {
            free_rects.push(FreeRect {
                x: f.x,
                y: f.y,
                w: f.w,
                h: by - f.y,
            });
        }
        if by2 < fy2 {
            free_rects.push(FreeRect {
                x: f.x,
                y: by2,
                w: f.w,
                h: fy2 - by2,
            });
        }
        free_rects[i].w = 0;
        i += 1;
    }

    // Drop zero-sized rects and any rect fully contained within another
    free_rects.retain(|r| r.w > 0 && r.h > 0);
    prune_contained(free_rects);
}

fn prune_contained(free_rects: &mut Vec<FreeRect>) {
    let mut i = 0;
    while i < free_rects.len() {
        let a = free_rects[i];
        let ax2 = a.x + a.w;
        let ay2 = a.y + a.h;
        let mut contained = false;
        let mut j = 0;
        while j < free_rects.len() {
            if i != j {
                let b = free_rects[j];
                if a.x >= b.x && a.y >= b.y && ax2 <= b.x + b.w && ay2 <= b.y + b.h {
                    contained = true;
                    break;
                }
            }
            j += 1;
        }
        if contained {
            free_rects.swap_remove(i);
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_of_two_helper() {
        assert_eq!(smallest_encompassing_power_of_two(0), 1);
        assert_eq!(smallest_encompassing_power_of_two(1), 1);
        assert_eq!(smallest_encompassing_power_of_two(15), 16);
        assert_eq!(smallest_encompassing_power_of_two(16), 16);
        assert_eq!(smallest_encompassing_power_of_two(17), 32);
        assert_eq!(smallest_encompassing_power_of_two(1000), 1024);
    }

    #[test]
    fn test_mixed_resolution_stitching() {
        let mut stitcher = Stitcher::new(4096, 4096, 0);

        // Register a mix of HD and vanilla resolution sprites
        stitcher.register_sprite("stone", 16, 16, "minecraft:block/stone");
        stitcher.register_sprite("dirt", 16, 16, "minecraft:block/dirt");
        stitcher.register_sprite("hd_sword", 128, 128, "minecraft:item/diamond_sword");
        stitcher.register_sprite("hd_water", 256, 256, "minecraft:block/water_still");
        stitcher.register_sprite("huge_block", 512, 512, "minecraft:block/huge");
        stitcher.register_sprite("tall_texture", 64, 128, "minecraft:block/door");

        let result = stitcher.stitch().unwrap();
        assert_eq!(result.chunks.len(), 1);

        let chunk = &result.chunks[0];
        assert!(chunk.width.is_power_of_two());
        assert!(chunk.height.is_power_of_two());
        assert_eq!(chunk.slots.len(), 6);

        // Verify non-overlapping layout
        for i in 0..chunk.slots.len() {
            let a = &chunk.slots[i];
            assert!(a.x + a.width <= chunk.width);
            assert!(a.y + a.height <= chunk.height);

            for j in (i + 1)..chunk.slots.len() {
                let b = &chunk.slots[j];
                let overlap_x = a.x < b.x + b.width && a.x + a.width > b.x;
                let overlap_y = a.y < b.y + b.height && a.y + a.height > b.y;
                assert!(
                    !(overlap_x && overlap_y),
                    "Slots '{}' and '{}' overlap!",
                    a.entry,
                    b.entry
                );
            }
        }
    }
}
