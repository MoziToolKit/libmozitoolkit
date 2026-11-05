use crate::error::TextureError;
use crate::stitcher::region::StitcherRegion;
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

/// Vanilla-style binary partitioning 2D Texture Stitcher.
pub(crate) struct Stitcher<T> {
    max_width: u32,
    max_height: u32,
    mip_level: u32,
    holders: Vec<StitcherHolder<T>>,
}

impl<T: Clone> Stitcher<T> {
    /// Create a new Stitcher with maximum atlas dimensions and mipmap level.
    pub fn new(max_width: u32, max_height: u32, mip_level: u32) -> Self {
        Self {
            max_width,
            max_height,
            mip_level,
            holders: Vec::new(),
        }
    }

    /// Register a sprite entry with its single-frame width, height, and canonical name.
    pub fn register_sprite(&mut self, entry: T, width: u32, height: u32, name: impl Into<String>) {
        // Enforce mipmap alignment: width and height must be aligned to 1 << mip_level
        let align = 1 << self.mip_level;
        let padded_w = if !width.is_multiple_of(align) {
            ((width / align) + 1) * align
        } else {
            width
        };
        let padded_h = if !height.is_multiple_of(align) {
            ((height / align) + 1) * align
        } else {
            height
        };

        self.holders.push(StitcherHolder {
            entry,
            width: padded_w,
            height: padded_h,
            name: name.into(),
        });
    }

    /// Execute the vanilla layout algorithm.
    /// Sorts entries by Height desc, Width desc, Name asc, then packs into binary partitioned regions.
    /// If entries exceed max dimensions, automatically allocates subsequent atlas chunks (pages).
    pub fn stitch(&mut self) -> Result<StitchedAtlas<T>, TextureError> {
        if self.holders.is_empty() {
            return Ok(StitchedAtlas { chunks: Vec::new() });
        }

        // 1. Sort holders strictly following Minecraft vanilla HOLDER_COMPARATOR:
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

        while !unplaced.is_empty() {
            let (chunk, remaining) = self.stitch_single_chunk(unplaced)?;
            chunks.push(chunk);
            unplaced = remaining;
        }

        Ok(StitchedAtlas { chunks })
    }

    fn stitch_single_chunk(
        &self,
        candidates: Vec<StitcherHolder<T>>,
    ) -> Result<(StitchedChunk<T>, Vec<StitcherHolder<T>>), TextureError> {
        let mut storage: Vec<StitcherRegion<StitcherHolder<T>>> = Vec::new();
        let mut storage_x = 0u32;
        let mut storage_y = 0u32;
        let mut unplaced = Vec::new();

        for holder in candidates {
            let w = holder.width;
            let h = holder.height;

            if w > self.max_width || h > self.max_height {
                return Err(TextureError::Stitcher(format!(
                    "Sprite '{}' dimensions ({}x{}) exceed maximum atlas size ({}x{})",
                    holder.name, w, h, self.max_width, self.max_height
                )));
            }

            // 1. Try placing into existing storage regions
            let mut placed = false;
            for region in storage.iter_mut() {
                if region.add(holder.clone(), w, h).is_none() {
                    placed = true;
                    break;
                }
            }

            if placed {
                continue;
            }

            // 2. Try expanding storage canvas
            if self.expand(&mut storage, &mut storage_x, &mut storage_y, &holder) {
                continue;
            }

            // 3. Cannot fit in current chunk -> push to unplaced for next chunk
            unplaced.push(holder);
        }

        // Collect all placed slots
        let mut slots = Vec::new();
        for region in &storage {
            region.walk(&mut |h, x, y, _rw, _rh| {
                slots.push(StitchedSlot {
                    entry: h.entry.clone(),
                    x,
                    y,
                    #[cfg(test)]
                    width: _rw,
                    #[cfg(test)]
                    height: _rh,
                });
            });
        }

        let final_w = smallest_encompassing_power_of_two(storage_x);
        let final_h = smallest_encompassing_power_of_two(storage_y);

        Ok((
            StitchedChunk {
                width: final_w,
                height: final_h,
                slots,
            },
            unplaced,
        ))
    }

    fn expand(
        &self,
        storage: &mut Vec<StitcherRegion<StitcherHolder<T>>>,
        storage_x: &mut u32,
        storage_y: &mut u32,
        holder: &StitcherHolder<T>,
    ) -> bool {
        let cur_pow_x = smallest_encompassing_power_of_two(*storage_x);
        let cur_pow_y = smallest_encompassing_power_of_two(*storage_y);
        let next_pow_x = smallest_encompassing_power_of_two(*storage_x + holder.width);
        let next_pow_y = smallest_encompassing_power_of_two(*storage_y + holder.height);

        let can_expand_x = next_pow_x <= self.max_width;
        let can_expand_y = next_pow_y <= self.max_height;

        if !can_expand_x && !can_expand_y {
            return false;
        }

        let is_expand_x_diff = can_expand_x && (cur_pow_x != next_pow_x);
        let is_expand_y_diff = can_expand_y && (cur_pow_y != next_pow_y);

        let expand_horizontal = if is_expand_x_diff ^ is_expand_y_diff {
            is_expand_x_diff
        } else {
            can_expand_x && cur_pow_x <= cur_pow_y
        };

        if expand_horizontal {
            if *storage_y == 0 {
                *storage_y = next_pow_y;
            }
            let mut new_region =
                StitcherRegion::new(*storage_x, 0, next_pow_x - *storage_x, *storage_y);
            *storage_x = next_pow_x;
            new_region.add(holder.clone(), holder.width, holder.height);
            storage.push(new_region);
        } else {
            if *storage_x == 0 {
                *storage_x = next_pow_x;
            }
            let mut new_region =
                StitcherRegion::new(0, *storage_y, *storage_x, next_pow_y - *storage_y);
            *storage_y = next_pow_y;
            new_region.add(holder.clone(), holder.width, holder.height);
            storage.push(new_region);
        }

        true
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
