//! Fluid height sampling, corner averaging, and flow vector physics.

use core::f32::consts::PI;
use mtk_cull::parse_block_name_and_props;

pub use mtk_core::constants::fluid::MAX_FLUID_HEIGHT;

/// Fast zero-allocation extraction of fluid properties from a blockstate string.
#[inline]
pub(crate) fn parse_fluid_info(state_str: &str) -> (Option<FluidType>, bool, u32) {
    if state_str.is_empty() {
        return (None, false, 0);
    }
    let clean_str = state_str.strip_prefix("minecraft:").unwrap_or(state_str);
    let (name, props_str) = match clean_str.find('[') {
        Some(idx) => (
            &clean_str[..idx],
            &clean_str[idx + 1..clean_str.len().saturating_sub(1)],
        ),
        None => (clean_str, ""),
    };

    let is_waterlogged = if props_str.contains("waterlogged=true") {
        true
    } else {
        name == "seagrass" || name == "tall_seagrass" || name == "kelp" || name == "kelp_plant"
    };

    let base_name = name.strip_prefix("flowing_").unwrap_or(name);
    let fluid_type = if base_name == "water" || is_waterlogged {
        Some(FluidType::Water)
    } else if base_name == "lava" {
        Some(FluidType::Lava)
    } else {
        None
    };

    let mut level = 0;
    if !props_str.is_empty() {
        if let Some(pos) = props_str.find("level=") {
            let after = &props_str[pos + 6..];
            let end = after.find(',').unwrap_or(after.len());
            if let Ok(lvl) = after[..end].parse::<u32>() {
                level = lvl;
            }
        }
    }

    (fluid_type, is_waterlogged, level)
}

/// Identifies supported Minecraft fluid types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FluidType {
    Water,
    Lava,
}

impl FluidType {
    pub fn from_name(name: &str) -> Option<Self> {
        let clean = name.strip_prefix("minecraft:").unwrap_or(name);
        let name_only = match clean.find('[') {
            Some(idx) => &clean[..idx],
            None => clean,
        };
        let clean = name_only.strip_prefix("flowing_").unwrap_or(name_only);
        if clean == "water" {
            Some(Self::Water)
        } else if clean == "lava" {
            Some(Self::Lava)
        } else {
            None
        }
    }

    pub fn name_str(&self) -> &'static str {
        match self {
            Self::Water => "water",
            Self::Lava => "lava",
        }
    }
}

/// Computes fluid height in `[0..1]` for a single fluid or waterlogged blockstate string.
pub(crate) fn get_fluid_base_height(state_str: &str) -> f32 {
    let (fluid, is_waterlogged, level_int) = parse_fluid_info(state_str);
    if is_waterlogged {
        return MAX_FLUID_HEIGHT;
    }
    if fluid.is_none() {
        return 0.0;
    }

    if level_int >= 8 {
        MAX_FLUID_HEIGHT
    } else if level_int > 0 {
        (8.0 - level_int as f32) / 9.0
    } else {
        MAX_FLUID_HEIGHT
    }
}

/// Samples fluid height at `(x, y, z)` relative to neighborhood:
/// - Returns `1.0` if same fluid and block directly above is also fluid/submerged.
/// - Returns own height in `(0..1)` if same fluid or waterlogged.
/// - Returns `-1.0` if solid opaque block (JMC2OBJ boundary preservation, excluded from averaging).
/// - Returns `0.0` if air or non-solid / non-fluid block.
pub(crate) fn sample_fluid_height<F>(
    mut get_state: F,
    x: i32,
    y: i32,
    z: i32,
    fluid_type: FluidType,
) -> f32
where
    F: FnMut(i32, i32, i32) -> String,
{
    let state_str = get_state(x, y, z);
    if state_str.is_empty() || state_str == "minecraft:air" || state_str == "air" {
        return 0.0;
    }

    let (f_type, is_waterlogged, level) = parse_fluid_info(&state_str);
    let is_fluid_match = f_type == Some(fluid_type);

    if is_fluid_match {
        // Check if block directly above is also the same fluid / waterlogged
        let above_state = get_state(x, y + 1, z);
        if !above_state.is_empty() && above_state != "minecraft:air" {
            let (a_type, _, _) = parse_fluid_info(&above_state);
            if a_type == Some(fluid_type) {
                return 1.0;
            }
        }

        if is_waterlogged && fluid_type == FluidType::Water {
            return MAX_FLUID_HEIGHT;
        }

        if level >= 8 {
            MAX_FLUID_HEIGHT
        } else if level > 0 {
            (8.0 - level as f32) / 9.0
        } else {
            MAX_FLUID_HEIGHT
        }
    } else {
        if is_waterlogged {
            return 0.0;
        }

        let clean = state_str.strip_prefix("minecraft:").unwrap_or(&state_str);
        let name = match clean.find('[') {
            Some(idx) => &clean[..idx],
            None => clean,
        };
        let is_solid_cube =
            mtk_model::is_opaque_block(name) && !mtk_model::is_known_non_cube(name, false);

        if is_solid_cube {
            return -1.0;
        }

        0.0
    }
}

/// Calculates the averaged fluid height for one corner between center, two adjacent neighbors,
/// and the diagonal neighbor using canonical Minecraft fluid mechanics.
pub(crate) fn calculate_corner_average(
    h_center: f32,
    h_adj1: f32,
    h_adj2: f32,
    h_diag: f32,
    is_source: bool,
) -> f32 {
    if h_center >= 1.0 || h_adj1 >= 1.0 || h_adj2 >= 1.0 {
        return 1.0;
    }

    // Still source block surface tension: maintain 8/9 unless bordering lower fluid
    if is_source {
        let samples = [h_adj1, h_adj2, h_diag];
        let mut count = 0;
        let mut all_ge_08 = true;
        for &h in &samples {
            if h > 0.0 {
                count += 1;
                if h < 0.8 {
                    all_ge_08 = false;
                }
            }
        }
        if count == 0 || all_ge_08 {
            return MAX_FLUID_HEIGHT;
        }
    }

    let mut weighted_sum = 0.0f32;
    let mut total_weight = 0.0f32;

    // 1. Center fluid
    if h_center >= 0.8 {
        weighted_sum += h_center * 10.0;
        total_weight += 10.0;
    } else if h_center >= 0.0 {
        weighted_sum += h_center * 1.0;
        total_weight += 1.0;
    }

    // 2. Adjacent 1
    if h_adj1 >= 0.8 {
        weighted_sum += h_adj1 * 10.0;
        total_weight += 10.0;
    } else if h_adj1 >= 0.0 {
        weighted_sum += h_adj1 * 1.0;
        total_weight += 1.0;
    }

    // 3. Adjacent 2
    if h_adj2 >= 0.8 {
        weighted_sum += h_adj2 * 10.0;
        total_weight += 10.0;
    } else if h_adj2 >= 0.0 {
        weighted_sum += h_adj2 * 1.0;
        total_weight += 1.0;
    }

    // 4. Diagonal (only if at least one adjacent neighbor is fluid)
    if (h_adj1 > 0.0 || h_adj2 > 0.0) && h_diag >= 0.0 {
        if h_diag >= 1.0 {
            return 1.0;
        }
        if h_diag >= 0.8 {
            weighted_sum += h_diag * 10.0;
            total_weight += 10.0;
        } else {
            weighted_sum += h_diag * 1.0;
            total_weight += 1.0;
        }
    }

    if total_weight > 0.0 {
        weighted_sum / total_weight
    } else {
        h_center.max(0.0)
    }
}

/// Computes 4 corner heights `(c_NW, c_NE, c_SE, c_SW)` in `[0..1]` for the fluid block at `(x, y, z)`.
/// North is -Z, South is +Z, West is -X, East is +X.
pub(crate) fn calculate_fluid_corner_heights<F>(
    mut get_state: F,
    x: i32,
    y: i32,
    z: i32,
    fluid_type: FluidType,
) -> (f32, f32, f32, f32)
where
    F: FnMut(i32, i32, i32) -> String,
{
    // Submerged check
    let above_state = get_state(x, y + 1, z);
    if !above_state.is_empty() && above_state != "minecraft:air" {
        let (a_type, _, _) = parse_fluid_info(&above_state);
        if a_type == Some(fluid_type) {
            return (1.0, 1.0, 1.0, 1.0);
        }
    }

    let state_str = get_state(x, y, z);
    let (_, is_waterlogged, level) = parse_fluid_info(&state_str);
    let is_source = is_waterlogged || (!state_str.contains("flowing_") && level == 0);

    let h_center = sample_fluid_height(&mut get_state, x, y, z, fluid_type);
    let h_n = sample_fluid_height(&mut get_state, x, y, z - 1, fluid_type);
    let h_s = sample_fluid_height(&mut get_state, x, y, z + 1, fluid_type);
    let h_e = sample_fluid_height(&mut get_state, x + 1, y, z, fluid_type);
    let h_w = sample_fluid_height(&mut get_state, x - 1, y, z, fluid_type);

    let h_ne = sample_fluid_height(&mut get_state, x + 1, y, z - 1, fluid_type);
    let h_nw = sample_fluid_height(&mut get_state, x - 1, y, z - 1, fluid_type);
    let h_se = sample_fluid_height(&mut get_state, x + 1, y, z + 1, fluid_type);
    let h_sw = sample_fluid_height(&mut get_state, x - 1, y, z + 1, fluid_type);

    let c_nw = calculate_corner_average(h_center, h_n, h_w, h_nw, is_source);
    let c_ne = calculate_corner_average(h_center, h_n, h_e, h_ne, is_source);
    let c_se = calculate_corner_average(h_center, h_s, h_e, h_se, is_source);
    let c_sw = calculate_corner_average(h_center, h_s, h_w, h_sw, is_source);

    (c_nw, c_ne, c_se, c_sw)
}

/// Calculates horizontal flow direction vector `(vx, vz)` and UV rotation angle in radians.
pub(crate) fn calculate_fluid_flow_vector<F>(
    mut get_state: F,
    x: i32,
    y: i32,
    z: i32,
    fluid_type: FluidType,
    own_height: f32,
) -> (f32, f32, f32)
where
    F: FnMut(i32, i32, i32) -> String,
{
    let mut vx = 0.0f32;
    let mut vz = 0.0f32;

    for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
        let nx = x + dx;
        let nz = z + dz;
        let n_state = get_state(nx, y, nz);

        if n_state.is_empty() || n_state == "minecraft:air" {
            // Check block below
            let below_state = get_state(nx, y - 1, nz);
            if !below_state.is_empty() && below_state != "minecraft:air" {
                let (b_name, b_props) = parse_block_name_and_props(&below_state);
                let b_clean = b_name.strip_prefix("minecraft:").unwrap_or(&b_name);
                let b_waterlogged = b_props.get("waterlogged").map(|v| v.as_str()) == Some("true");
                let b_clean = b_clean.strip_prefix("flowing_").unwrap_or(b_clean);

                if (b_clean == fluid_type.name_str())
                    || (fluid_type == FluidType::Water && b_waterlogged)
                {
                    let b_h = get_fluid_base_height(&below_state);
                    let diff = own_height - (b_h - MAX_FLUID_HEIGHT);
                    vx += dx as f32 * diff;
                    vz += dz as f32 * diff;
                }
            }
        } else {
            let (n_name, n_props) = parse_block_name_and_props(&n_state);
            let n_clean = n_name.strip_prefix("minecraft:").unwrap_or(&n_name);
            let n_waterlogged = n_props.get("waterlogged").map(|v| v.as_str()) == Some("true");
            let n_clean = n_clean.strip_prefix("flowing_").unwrap_or(n_clean);

            if (n_clean == fluid_type.name_str())
                || (fluid_type == FluidType::Water && n_waterlogged)
            {
                let n_h = get_fluid_base_height(&n_state);
                let diff = own_height - n_h;
                if diff.abs() > 1e-4 {
                    vx += dx as f32 * diff;
                    vz += dz as f32 * diff;
                }
            }
        }
    }

    let flow_len = (vx * vx + vz * vz).sqrt();
    if flow_len < 1e-4 {
        return (0.0, 0.0, 0.0);
    }

    let flow_angle = vz.atan2(vx) - (PI / 2.0);
    (vx, vz, flow_angle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fluid_base_heights() {
        assert_eq!(get_fluid_base_height("minecraft:water"), MAX_FLUID_HEIGHT);
        assert_eq!(
            get_fluid_base_height("minecraft:water[level=0]"),
            MAX_FLUID_HEIGHT
        );
        assert_eq!(
            get_fluid_base_height("minecraft:flowing_water[level=1]"),
            7.0 / 9.0
        );
        assert_eq!(
            get_fluid_base_height("minecraft:flowing_water[level=7]"),
            1.0 / 9.0
        );
        assert_eq!(
            get_fluid_base_height("minecraft:oak_stairs[waterlogged=true]"),
            MAX_FLUID_HEIGHT
        );
    }

    #[test]
    fn test_still_source_surface_tension() {
        // A still source water surrounded by air should stay at MAX_FLUID_HEIGHT (8/9) without drooping
        let corner_h = calculate_corner_average(MAX_FLUID_HEIGHT, 0.0, 0.0, 0.0, true);
        assert_eq!(corner_h, MAX_FLUID_HEIGHT);
    }

    #[test]
    fn test_fluid_corner_height_calculation() {
        let get_state = |x: i32, y: i32, z: i32| -> String {
            if x == 0 && y == 64 && z == 0 {
                "minecraft:water".to_string()
            } else {
                "minecraft:air".to_string()
            }
        };

        let (c_nw, c_ne, c_se, c_sw) =
            calculate_fluid_corner_heights(get_state, 0, 64, 0, FluidType::Water);

        let eps = 1e-4;
        assert!((c_nw - MAX_FLUID_HEIGHT).abs() < eps);
        assert!((c_ne - MAX_FLUID_HEIGHT).abs() < eps);
        assert!((c_se - MAX_FLUID_HEIGHT).abs() < eps);
        assert!((c_sw - MAX_FLUID_HEIGHT).abs() < eps);
    }
}
