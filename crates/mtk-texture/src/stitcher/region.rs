/// A 2D rectangular space partitioning node inside the Stitcher.
#[derive(Debug, Clone)]
pub(crate) struct StitcherRegion<T> {
    pub(crate) origin_x: u32,
    pub(crate) origin_y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) holder: Option<T>,
    pub(crate) sub_slots: Option<Vec<StitcherRegion<T>>>,
}

impl<T> StitcherRegion<T> {
    pub(crate) fn new(origin_x: u32, origin_y: u32, width: u32, height: u32) -> Self {
        Self {
            origin_x,
            origin_y,
            width,
            height,
            holder: None,
            sub_slots: None,
        }
    }

    /// Try to add a slot into this region or its partitioned sub-slots.
    pub fn add(&mut self, holder: T, holder_width: u32, holder_height: u32) -> Option<T> {
        if self.holder.is_some() {
            return Some(holder);
        }

        if let Some(ref mut slots) = self.sub_slots {
            let mut curr = holder;
            for slot in slots.iter_mut() {
                {
                    let rem = slot.add(curr, holder_width, holder_height)?;
                    curr = rem;
                }
            }
            return Some(curr);
        }

        // Check if this region can fit the holder
        if self.width >= holder_width && self.height >= holder_height {
            let dx = self.width - holder_width;
            let dy = self.height - holder_height;

            if dx == 0 && dy == 0 {
                self.holder = Some(holder);
                return None;
            }

            let mut sub_slots = Vec::with_capacity(2);
            if dx > dy {
                // Split horizontally: left slot (exact width) and right remainder
                sub_slots.push(StitcherRegion::new(
                    self.origin_x,
                    self.origin_y,
                    holder_width,
                    self.height,
                ));
                sub_slots.push(StitcherRegion::new(
                    self.origin_x + holder_width,
                    self.origin_y,
                    dx,
                    self.height,
                ));
            } else {
                // Split vertically: top slot (exact height) and bottom remainder
                sub_slots.push(StitcherRegion::new(
                    self.origin_x,
                    self.origin_y,
                    self.width,
                    holder_height,
                ));
                sub_slots.push(StitcherRegion::new(
                    self.origin_x,
                    self.origin_y + holder_height,
                    self.width,
                    dy,
                ));
            }

            // Insert into first sub-slot
            let rem = sub_slots[0].add(holder, holder_width, holder_height);
            self.sub_slots = Some(sub_slots);
            rem
        } else {
            Some(holder)
        }
    }

    /// Walk all placed holders and invoke the callback with their top-left coordinates.
    pub fn walk<F: FnMut(&T, u32, u32, u32, u32)>(&self, callback: &mut F) {
        if let Some(ref h) = self.holder {
            callback(h, self.origin_x, self.origin_y, self.width, self.height);
        } else if let Some(ref slots) = self.sub_slots {
            for slot in slots {
                slot.walk(callback);
            }
        }
    }
}
