/// 47 Full CTM tile index to connection bit pattern.
pub(crate) const TILE_TO_CONNECTION_DATA: [u8; 47] = [
    0b00000000, 0b00010000, 0b00010001, 0b00000001, 0b00010100, 0b00000101, 0b01010100, 0b00010101,
    0b01110101, 0b01011101, 0b11010111, 0b11110101, 0b00000100, 0b00011100, 0b00011111, 0b00000111,
    0b01010000, 0b01000001, 0b01010001, 0b01000101, 0b11010101, 0b01010111, 0b01011111, 0b01111101,
    0b01000100, 0b01111100, 0b11111111, 0b11000111, 0b01011100, 0b00010111, 0b01110100, 0b00011101,
    0b11110111, 0b11111101, 0b01110111, 0b11011101, 0b01000000, 0b01110000, 0b11110001, 0b11000001,
    0b01110001, 0b11000101, 0b11010001, 0b01000111, 0b11011111, 0b01111111, 0b01010101,
];

/// Precomputed 256-entry lookup table for Full 47-tile CTM.
pub(crate) const fn build_ctm_47_lookup() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut mapped = [false; 256];

    let mut i = 0;
    while i < 47 {
        let pattern = TILE_TO_CONNECTION_DATA[i] as usize;
        table[pattern] = i as u8;
        mapped[pattern] = true;
        i += 1;
    }

    let mut pattern = 0;
    while pattern < 256 {
        if !mapped[pattern] {
            let mut tile_idx = pattern;
            let mut corner_bit = 1;
            while corner_bit < 8 {
                let left_side_bit = if corner_bit == 0 { 7 } else { corner_bit - 1 };
                let right_side_bit = if corner_bit + 1 >= 8 {
                    0
                } else {
                    corner_bit + 1
                };

                let left_side = tile_idx & (1 << left_side_bit);
                let right_side = tile_idx & (1 << right_side_bit);

                if left_side == 0 || right_side == 0 {
                    tile_idx &= !(1 << corner_bit);
                }
                corner_bit += 2;
            }

            table[pattern] = table[tile_idx];
        }
        pattern += 1;
    }

    table
}

pub(crate) const CTM_47_LOOKUP: [u8; 256] = build_ctm_47_lookup();

/// 17 Overlay tile index to connection bit pattern.
pub(crate) const TILE_TO_OVERLAY_DATA: [u8; 17] = [
    0b00001000, 0b00001110, 0b00000010, 0b00111110, 0b10001111, 0b10111111, 0b11101111, 0b00111000,
    0b11111111, 0b10000011, 0b11111000, 0b11100011, 0b11111110, 0b11111011, 0b00100000, 0b11100000,
    0b10000000,
];

/// Precomputed 256-entry lookup table for Overlay CTM.
pub(crate) const fn build_overlay_17_lookup() -> [i8; 256] {
    let mut table = [-2i8; 256];
    table[0b00000000] = -1;

    let mut i = 0;
    while i < 17 {
        table[TILE_TO_OVERLAY_DATA[i] as usize] = i as i8;
        i += 1;
    }

    table[0b11101110] = 1;
    table[0b10111011] = 7;

    let mut pattern = 0;
    while pattern < 256 {
        if table[pattern] < -1 {
            let mut tile_idx = pattern;
            let mut corner_bit = 1;
            while corner_bit < 8 {
                let left_side_bit = if corner_bit == 0 { 7 } else { corner_bit - 1 };
                let right_side_bit = if corner_bit + 1 >= 8 {
                    0
                } else {
                    corner_bit + 1
                };

                let left_side = tile_idx & (1 << left_side_bit);
                let right_side = tile_idx & (1 << right_side_bit);

                if left_side > 0 || right_side > 0 {
                    tile_idx |= 1 << corner_bit;
                }
                if left_side == 0 && right_side == 0 {
                    tile_idx &= !(1 << corner_bit);
                }
                corner_bit += 2;
            }
            table[pattern] = table[tile_idx];
        }
        pattern += 1;
    }

    table
}

pub(crate) const OVERLAY_17_LOOKUP: [i8; 256] = build_overlay_17_lookup();
