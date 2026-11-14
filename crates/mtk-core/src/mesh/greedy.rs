#![forbid(unsafe_code)]
//! # Generic 2D & 1D Greedy Meshing Engine
//!
//! Provides high-performance, deterministic rectangular greedy merging for 2D binary masks,
//! 2D paletted/attribute grids, and 1D outline spans.
//!
//! Widely consumed by:
//! - 2D Extruded Item Model Bakers (`mtk-model::item`) to drastically reduce quad counts
//!   for front, back, and perimeter side walls;
//! - Voxel scene slice meshing and coplanar block surface merges (`mtk-voxel`).

use alloc::vec;
use alloc::vec::Vec;

/// A merged 2D integer rectangle with top-left origin `(x, y)` and dimensions `(w, h)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GreedyRect {
    /// Column start index (0-based, horizontal).
    pub x: u32,
    /// Row start index (0-based, vertical).
    pub y: u32,
    /// Width in cells (horizontal span >= 1).
    pub w: u32,
    /// Height in cells (vertical span >= 1).
    pub h: u32,
}

impl GreedyRect {
    /// Creates a new greedy rectangle.
    #[inline]
    pub const fn new(x: u32, y: u32, w: u32, h: u32) -> Self {
        Self { x, y, w, h }
    }

    /// Area in cells.
    #[inline]
    pub const fn area(&self) -> u32 {
        self.w * self.h
    }
}

/// Merges a 2D binary mask (`true` = solid, `false` = empty) into a minimal disjoint set of `GreedyRect`s.
///
/// * `mask`: Row-major flat array of length `(width * height)`.
/// * `width`: Grid width in cells.
/// * `height`: Grid height in cells.
pub fn greedy_mesh_2d_mask(mask: &[bool], width: u32, height: u32) -> Vec<GreedyRect> {
    let total = (width as usize) * (height as usize);
    if total == 0 || mask.len() < total {
        return Vec::new();
    }

    let mut visited = vec![false; total];
    let mut rects = Vec::new();

    let w_usize = width as usize;
    let h_usize = height as usize;

    for y in 0..h_usize {
        for x in 0..w_usize {
            let idx = y * w_usize + x;
            if visited[idx] || !mask[idx] {
                continue;
            }

            // 1. Extend horizontally as far as possible
            let mut rect_w = 1;
            while x + rect_w < w_usize {
                let next_idx = y * w_usize + (x + rect_w);
                if !visited[next_idx] && mask[next_idx] {
                    rect_w += 1;
                } else {
                    break;
                }
            }

            // 2. Extend vertically as far as the entire row interval [x .. x + rect_w] is uniform and unvisited
            let mut rect_h = 1;
            'vertical: while y + rect_h < h_usize {
                let row_start = (y + rect_h) * w_usize + x;
                for col in 0..rect_w {
                    let check_idx = row_start + col;
                    if visited[check_idx] || !mask[check_idx] {
                        break 'vertical;
                    }
                }
                rect_h += 1;
            }

            // 3. Mark the whole rectangular region as visited
            for dy in 0..rect_h {
                let row_start = (y + dy) * w_usize + x;
                for dx in 0..rect_w {
                    visited[row_start + dx] = true;
                }
            }

            rects.push(GreedyRect::new(
                x as u32,
                y as u32,
                rect_w as u32,
                rect_h as u32,
            ));
        }
    }

    rects
}

/// Merges a 2D paletted or attribute-bearing grid into disjoint rectangular patches with uniform value.
///
/// Empty cells (`None`) are skipped.
pub fn greedy_mesh_2d_values<T: Eq + Clone>(
    grid: &[Option<T>],
    width: u32,
    height: u32,
) -> Vec<(GreedyRect, T)> {
    let total = (width as usize) * (height as usize);
    if total == 0 || grid.len() < total {
        return Vec::new();
    }

    let mut visited = vec![false; total];
    let mut rects = Vec::new();

    let w_usize = width as usize;
    let h_usize = height as usize;

    for y in 0..h_usize {
        for x in 0..w_usize {
            let idx = y * w_usize + x;
            if visited[idx] {
                continue;
            }
            let target_val = match &grid[idx] {
                Some(v) => v,
                None => continue,
            };

            // 1. Extend horizontally
            let mut rect_w = 1;
            while x + rect_w < w_usize {
                let next_idx = y * w_usize + (x + rect_w);
                if !visited[next_idx] {
                    if let Some(val) = &grid[next_idx] {
                        if val == target_val {
                            rect_w += 1;
                            continue;
                        }
                    }
                }
                break;
            }

            // 2. Extend vertically
            let mut rect_h = 1;
            'vertical: while y + rect_h < h_usize {
                let row_start = (y + rect_h) * w_usize + x;
                for col in 0..rect_w {
                    let check_idx = row_start + col;
                    if visited[check_idx] {
                        break 'vertical;
                    }
                    match &grid[check_idx] {
                        Some(val) if val == target_val => {}
                        _ => break 'vertical,
                    }
                }
                rect_h += 1;
            }

            // 3. Mark region visited
            for dy in 0..rect_h {
                let row_start = (y + dy) * w_usize + x;
                for dx in 0..rect_w {
                    visited[row_start + dx] = true;
                }
            }

            rects.push((
                GreedyRect::new(x as u32, y as u32, rect_w as u32, rect_h as u32),
                target_val.clone(),
            ));
        }
    }

    rects
}

/// Merges a 1D binary mask into continuous spans of `true` values.
///
/// Returns a list of `(start_index, length)` pairs.
#[inline]
pub fn greedy_span_1d(mask: &[bool]) -> Vec<(u32, u32)> {
    let mut spans = Vec::new();
    let mut i = 0;
    let len = mask.len();

    while i < len {
        if mask[i] {
            let start = i;
            while i < len && mask[i] {
                i += 1;
            }
            spans.push((start as u32, (i - start) as u32));
        } else {
            i += 1;
        }
    }

    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_greedy_mesh_empty_and_zero_size() {
        assert_eq!(greedy_mesh_2d_mask(&[], 0, 0), vec![]);
        assert_eq!(greedy_mesh_2d_mask(&[true; 4], 0, 4), vec![]);
        assert_eq!(greedy_mesh_2d_mask(&[false; 16], 4, 4), vec![]);
    }

    #[test]
    fn test_greedy_mesh_full_solid() {
        let mask = vec![true; 16]; // 4x4
        let rects = greedy_mesh_2d_mask(&mask, 4, 4);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0], GreedyRect::new(0, 0, 4, 4));
    }

    #[test]
    fn test_greedy_mesh_single_pixel() {
        let mut mask = vec![false; 16];
        mask[5] = true; // (x=1, y=1) in 4x4
        let rects = greedy_mesh_2d_mask(&mask, 4, 4);
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0], GreedyRect::new(1, 1, 1, 1));
    }

    #[test]
    fn test_greedy_mesh_checkerboard() {
        // 4x4 checkerboard has 8 isolated single-cell islands
        let mut mask = vec![false; 16];
        for y in 0..4 {
            for x in 0..4 {
                if (x + y) % 2 == 0 {
                    mask[y * 4 + x] = true;
                }
            }
        }
        let rects = greedy_mesh_2d_mask(&mask, 4, 4);
        assert_eq!(rects.len(), 8);
        for r in rects {
            assert_eq!(r.w, 1);
            assert_eq!(r.h, 1);
        }
    }

    #[test]
    fn test_greedy_mesh_l_shape() {
        // 3x3 L-shape:
        // 1 0 0
        // 1 0 0
        // 1 1 1
        let mask = vec![
            true, false, false, //
            true, false, false, //
            true, true, true, //
        ];
        let rects = greedy_mesh_2d_mask(&mask, 3, 3);
        // Greedy horizontal extension on row 0 picks (0,0) with w=1, extending down to (0,2) h=3
        // then row 2 has (1,2) with w=2, h=1.
        // Total cells covered = 3 + 2 = 5 cells.
        let total_area: u32 = rects.iter().map(|r| r.area()).sum();
        assert_eq!(total_area, 5);
        assert_eq!(rects.len(), 2);
    }

    #[test]
    fn test_greedy_mesh_donut_hole() {
        // 3x3 with center hole:
        // 1 1 1
        // 1 0 1
        // 1 1 1
        let mask = vec![
            true, true, true, //
            true, false, true, //
            true, true, true, //
        ];
        let rects = greedy_mesh_2d_mask(&mask, 3, 3);
        let total_area: u32 = rects.iter().map(|r| r.area()).sum();
        assert_eq!(total_area, 8);
        // (0,0) w=3, h=1 (3)
        // (0,1) w=1, h=1 (1)
        // (2,1) w=1, h=1 (1)
        // (0,2) w=3, h=1 (3)
        // -> 4 rects
        assert_eq!(rects.len(), 4);
    }

    #[test]
    fn test_greedy_mesh_values_partitions_distinct_colors() {
        // 2x2 grid with two colors A and B:
        // A B
        // A B
        let grid = vec![
            Some("A"),
            Some("B"), //
            Some("A"),
            Some("B"), //
        ];
        let rects = greedy_mesh_2d_values(&grid, 2, 2);
        assert_eq!(rects.len(), 2);
        assert_eq!(rects[0], (GreedyRect::new(0, 0, 1, 2), "A"));
        assert_eq!(rects[1], (GreedyRect::new(1, 0, 1, 2), "B"));
    }

    #[test]
    fn test_greedy_span_1d_cases() {
        assert_eq!(greedy_span_1d(&[]), vec![]);
        assert_eq!(greedy_span_1d(&[false, false]), vec![]);
        assert_eq!(greedy_span_1d(&[true, true, true]), vec![(0, 3)]);
        assert_eq!(
            greedy_span_1d(&[false, true, true, false, true]),
            vec![(1, 2), (4, 1)]
        );
    }
}
