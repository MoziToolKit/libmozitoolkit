//! Procedural geometric model generator for Minecraft 1.21.4+ / 26.2 Copper Golem Statues.
//!
//! Mirrors `CopperGolemStatueSpecialRenderer` and `CopperGolemModel` from vanilla client,
//! supporting 4 oxidation states (plus waxed variants) and 4 statue poses
//! (`standing`, `sitting`, `running`, `star`), oriented to any horizontal facing.

use std::collections::HashMap;

use glam::{Mat4, Vec2, Vec3};
use mtk_core::Direction;

use crate::baker::baked_model::model::{BakedElement, BakedFace, BakedModel};
use crate::parser::blockstate::BlockState;

/// Oxidation weathering level of a copper golem statue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CopperOxidation {
    Unaffected,
    Exposed,
    Weathered,
    Oxidized,
}

impl CopperOxidation {
    /// Resolves oxidation state from blockstate name (e.g. `"minecraft:waxed_weathered_copper_golem_statue"`).
    pub fn from_block_name(name: &str) -> Self {
        let clean = name.strip_prefix("minecraft:").unwrap_or(name);
        if clean.contains("exposed") {
            Self::Exposed
        } else if clean.contains("weathered") {
            Self::Weathered
        } else if clean.contains("oxidized") {
            Self::Oxidized
        } else {
            Self::Unaffected
        }
    }

    /// Vanilla texture asset path for this oxidation state.
    pub fn texture_path(&self) -> &'static str {
        match self {
            Self::Unaffected => "minecraft:entity/copper_golem/copper_golem",
            Self::Exposed => "minecraft:entity/copper_golem/copper_golem_exposed",
            Self::Weathered => "minecraft:entity/copper_golem/copper_golem_weathered",
            Self::Oxidized => "minecraft:entity/copper_golem/copper_golem_oxidized",
        }
    }
}

/// The 4 statue poses supported in Minecraft 1.21.4+ / 26.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CopperStatuePose {
    Standing,
    Sitting,
    Running,
    Star,
}

impl CopperStatuePose {
    pub fn parse(s: &str) -> Self {
        match s {
            "sitting" => Self::Sitting,
            "running" => Self::Running,
            "star" => Self::Star,
            _ => Self::Standing,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standing => "standing",
            Self::Sitting => "sitting",
            Self::Running => "running",
            Self::Star => "star",
        }
    }
}

/// Local cuboid box definition with 64x64 texture offsets.
#[derive(Debug, Clone)]
struct ModelBox {
    from: Vec3,
    size: Vec3,
    tex_offset: [f32; 2],
}

/// Hierarchical model part definition.
#[derive(Debug, Clone)]
struct PartDef {
    offset: Vec3,
    rotation: Vec3, // (rx, ry, rz) radians
    parent: Option<usize>,
    boxes: Vec<ModelBox>,
}

/// Constructs the parts hierarchy for the given pose.
fn build_parts_for_pose(pose: CopperStatuePose) -> Vec<PartDef> {
    match pose {
        CopperStatuePose::Standing => vec![
            // 0: Root
            PartDef {
                offset: Vec3::ZERO,
                rotation: Vec3::ZERO,
                parent: None,
                boxes: Vec::new(),
            },
            // 1: Body
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox {
                    from: Vec3::new(-4.0, -6.0, -3.0),
                    size: Vec3::new(8.0, 6.0, 6.0),
                    tex_offset: [0.0, 15.0],
                }],
            },
            // 2: Head (child of body)
            PartDef {
                offset: Vec3::new(0.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    // Head main
                    ModelBox {
                        from: Vec3::new(-4.0, -5.0, -5.0),
                        size: Vec3::new(8.0, 5.0, 10.0),
                        tex_offset: [0.0, 0.0],
                    },
                    // Nose
                    ModelBox {
                        from: Vec3::new(-1.0, -2.0, -6.0),
                        size: Vec3::new(2.0, 3.0, 2.0),
                        tex_offset: [56.0, 0.0],
                    },
                    // Antenna rod
                    ModelBox {
                        from: Vec3::new(-1.0, -9.0, -1.0),
                        size: Vec3::new(2.0, 4.0, 2.0),
                        tex_offset: [37.0, 8.0],
                    },
                    // Antenna bulb
                    ModelBox {
                        from: Vec3::new(-2.0, -13.0, -2.0),
                        size: Vec3::new(4.0, 4.0, 4.0),
                        tex_offset: [37.0, 0.0],
                    },
                ],
            },
            // 3: Right Arm (child of body)
            PartDef {
                offset: Vec3::new(-4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![ModelBox {
                    from: Vec3::new(-3.0, -1.0, -2.0),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [36.0, 16.0],
                }],
            },
            // 4: Left Arm (child of body)
            PartDef {
                offset: Vec3::new(4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![ModelBox {
                    from: Vec3::new(0.0, -1.0, -2.0),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [50.0, 16.0],
                }],
            },
            // 5: Right Leg (child of root)
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox {
                    from: Vec3::new(-4.0, 0.0, -2.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [0.0, 27.0],
                }],
            },
            // 6: Left Leg (child of root)
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox {
                    from: Vec3::new(0.0, 0.0, -2.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [16.0, 27.0],
                }],
            },
        ],

        CopperStatuePose::Running => vec![
            // 0: Root
            PartDef {
                offset: Vec3::ZERO,
                rotation: Vec3::ZERO,
                parent: None,
                boxes: Vec::new(),
            },
            // 1: Body
            PartDef {
                offset: Vec3::new(-1.064, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 2: Body_r1 (child of body)
            PartDef {
                offset: Vec3::new(1.1, 0.1, 0.7),
                rotation: Vec3::new(0.1204, -0.0064, -0.0779),
                parent: Some(1),
                boxes: vec![ModelBox {
                    from: Vec3::new(-4.02, -6.116, -3.5),
                    size: Vec3::new(8.0, 6.0, 6.0),
                    tex_offset: [0.0, 15.0],
                }],
            },
            // 3: Head (child of body)
            PartDef {
                offset: Vec3::new(0.7, -5.6, -1.8),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox {
                        from: Vec3::new(-4.0, -5.1, -5.0),
                        size: Vec3::new(8.0, 5.0, 10.0),
                        tex_offset: [0.0, 0.0],
                    },
                    ModelBox {
                        from: Vec3::new(-1.02, -2.1, -6.0),
                        size: Vec3::new(2.0, 3.0, 2.0),
                        tex_offset: [56.0, 0.0],
                    },
                    ModelBox {
                        from: Vec3::new(-1.02, -9.1, -1.0),
                        size: Vec3::new(2.0, 4.0, 2.0),
                        tex_offset: [37.0, 8.0],
                    },
                    ModelBox {
                        from: Vec3::new(-2.0, -13.1, -2.0),
                        size: Vec3::new(4.0, 4.0, 4.0),
                        tex_offset: [37.0, 0.0],
                    },
                ],
            },
            // 4: Right Arm (child of body)
            PartDef {
                offset: Vec3::new(-4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: Vec::new(),
            },
            // 5: Right Arm_r1 (child of Right Arm)
            PartDef {
                offset: Vec3::new(0.7, -0.248, -1.62),
                rotation: Vec3::new(1.0036, 0.0, 0.0),
                parent: Some(4),
                boxes: vec![ModelBox {
                    from: Vec3::new(-3.052, -1.11, -2.036),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [36.0, 16.0],
                }],
            },
            // 6: Left Arm (child of body)
            PartDef {
                offset: Vec3::new(4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: Vec::new(),
            },
            // 7: Left Arm_r1 (child of Left Arm)
            PartDef {
                offset: Vec3::new(0.732, 0.0, 0.0),
                rotation: Vec3::new(-0.8715, -0.0535, -0.0449),
                parent: Some(6),
                boxes: vec![ModelBox {
                    from: Vec3::new(0.032, -1.1, -2.0),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [50.0, 16.0],
                }],
            },
            // 8: Right Leg (child of root)
            PartDef {
                offset: Vec3::new(-3.064, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 9: Right Leg_r1 (child of Right Leg)
            PartDef {
                offset: Vec3::new(1.048, 0.0, -0.9),
                rotation: Vec3::new(-0.8727, 0.0, 0.0),
                parent: Some(8),
                boxes: vec![ModelBox {
                    from: Vec3::new(-1.856, -0.1, -1.09),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [0.0, 27.0],
                }],
            },
            // 10: Left Leg (child of root)
            PartDef {
                offset: Vec3::new(0.936, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 11: Left Leg_r1 (child of Left Leg)
            PartDef {
                offset: Vec3::new(1.0, 0.0, 0.0),
                rotation: Vec3::new(std::f32::consts::FRAC_PI_4, 0.0, 0.0),
                parent: Some(10),
                boxes: vec![ModelBox {
                    from: Vec3::new(-2.088, -0.1, -2.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [16.0, 27.0],
                }],
            },
        ],

        CopperStatuePose::Sitting => vec![
            // 0: Root
            PartDef {
                offset: Vec3::ZERO,
                rotation: Vec3::ZERO,
                parent: None,
                boxes: Vec::new(),
            },
            // 1: Body
            PartDef {
                offset: Vec3::new(0.0, -3.0, 2.325),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![
                    ModelBox {
                        from: Vec3::new(-3.0, -4.0, -4.525),
                        size: Vec3::new(6.0, 1.0, 6.0),
                        tex_offset: [3.0, 19.0],
                    },
                    ModelBox {
                        from: Vec3::new(-4.0, -3.0, -3.525),
                        size: Vec3::new(8.0, 6.0, 6.0),
                        tex_offset: [0.0, 15.0],
                    },
                ],
            },
            // 2: Body_r1 (child of body)
            PartDef {
                offset: Vec3::new(0.0, -1.0, -4.325),
                rotation: Vec3::new(0.0, 0.0, -std::f32::consts::PI),
                parent: Some(1),
                boxes: vec![ModelBox {
                    from: Vec3::new(-4.0, -3.0, -2.2),
                    size: Vec3::new(8.0, 6.0, 3.0),
                    tex_offset: [3.0, 18.0],
                }],
            },
            // 3: Head (child of body)
            PartDef {
                offset: Vec3::new(0.0, -6.0, -0.2),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox {
                        from: Vec3::new(-1.0, -7.0, -3.3),
                        size: Vec3::new(2.0, 4.0, 2.0),
                        tex_offset: [37.0, 8.0],
                    },
                    ModelBox {
                        from: Vec3::new(-2.0, -11.0, -4.3),
                        size: Vec3::new(4.0, 4.0, 4.0),
                        tex_offset: [37.0, 0.0],
                    },
                    ModelBox {
                        from: Vec3::new(-4.0, -3.0, -7.325),
                        size: Vec3::new(8.0, 5.0, 10.0),
                        tex_offset: [0.0, 0.0],
                    },
                    ModelBox {
                        from: Vec3::new(-1.0, 0.0, -8.325),
                        size: Vec3::new(2.0, 3.0, 2.0),
                        tex_offset: [56.0, 0.0],
                    },
                ],
            },
            // 4: Right Arm (child of body)
            PartDef {
                offset: Vec3::new(-4.0, -5.6, -1.8),
                rotation: Vec3::new(0.4363, 0.0, 0.0),
                parent: Some(1),
                boxes: Vec::new(),
            },
            // 5: Right Arm_r1
            PartDef {
                offset: Vec3::new(0.0, 0.0893, 0.1198),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_3, 0.0, 0.0),
                parent: Some(4),
                boxes: vec![ModelBox {
                    from: Vec3::new(-3.075, -0.9733, -1.9966),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [36.0, 16.0],
                }],
            },
            // 6: Left Arm (child of body)
            PartDef {
                offset: Vec3::new(4.0, -5.6, -1.7),
                rotation: Vec3::new(0.4363, 0.0, 0.0),
                parent: Some(1),
                boxes: Vec::new(),
            },
            // 7: Left Arm_r1
            PartDef {
                offset: Vec3::new(0.0, -0.0015, -0.0808),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_3, 0.0, 0.0),
                parent: Some(6),
                boxes: vec![ModelBox {
                    from: Vec3::new(0.075, -1.0443, -1.8997),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [50.0, 16.0],
                }],
            },
            // 8: Right Leg (child of root)
            PartDef {
                offset: Vec3::new(-2.1, -2.1, -2.075),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 9: Right Leg_r1
            PartDef {
                offset: Vec3::new(0.05, -1.9, 1.075),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0),
                parent: Some(8),
                boxes: vec![ModelBox {
                    from: Vec3::new(-2.0, 0.975, 0.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [0.0, 27.0],
                }],
            },
            // 10: Left Leg (child of root)
            PartDef {
                offset: Vec3::new(2.0, -2.0, -2.075),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 11: Left Leg_r1
            PartDef {
                offset: Vec3::new(0.05, -2.0, 1.075),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0),
                parent: Some(10),
                boxes: vec![ModelBox {
                    from: Vec3::new(-2.0, 0.975, 0.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [16.0, 27.0],
                }],
            },
        ],

        CopperStatuePose::Star => vec![
            // 0: Root
            PartDef {
                offset: Vec3::ZERO,
                rotation: Vec3::ZERO,
                parent: None,
                boxes: Vec::new(),
            },
            // 1: Body
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox {
                    from: Vec3::new(-4.0, -6.0, -3.0),
                    size: Vec3::new(8.0, 6.0, 6.0),
                    tex_offset: [0.0, 15.0],
                }],
            },
            // 2: Head (child of body)
            PartDef {
                offset: Vec3::new(0.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox {
                        from: Vec3::new(-4.0, -5.0, -5.0),
                        size: Vec3::new(8.0, 5.0, 10.0),
                        tex_offset: [0.0, 0.0],
                    },
                    ModelBox {
                        from: Vec3::new(-1.0, -2.0, -6.0),
                        size: Vec3::new(2.0, 3.0, 2.0),
                        tex_offset: [56.0, 0.0],
                    },
                    ModelBox {
                        from: Vec3::new(-1.0, -9.0, -1.0),
                        size: Vec3::new(2.0, 4.0, 2.0),
                        tex_offset: [37.0, 8.0],
                    },
                    ModelBox {
                        from: Vec3::new(-2.0, -13.0, -2.0),
                        size: Vec3::new(4.0, 4.0, 4.0),
                        tex_offset: [37.0, 0.0],
                    },
                ],
            },
            // 3: Right Arm (child of body)
            PartDef {
                offset: Vec3::new(-4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: Vec::new(),
            },
            // 4: Right Arm_r1
            PartDef {
                offset: Vec3::new(1.0, 1.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, 1.9199),
                parent: Some(3),
                boxes: vec![ModelBox {
                    from: Vec3::new(-1.5, -5.0, -2.0),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [36.0, 16.0],
                }],
            },
            // 5: Left Arm (child of body)
            PartDef {
                offset: Vec3::new(4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: Vec::new(),
            },
            // 6: Left Arm_r1
            PartDef {
                offset: Vec3::new(-1.0, 1.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, -1.9199),
                parent: Some(5),
                boxes: vec![ModelBox {
                    from: Vec3::new(-1.5, -5.0, -2.0),
                    size: Vec3::new(3.0, 10.0, 4.0),
                    tex_offset: [50.0, 16.0],
                }],
            },
            // 7: Right Leg (child of root)
            PartDef {
                offset: Vec3::new(-3.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 8: Right Leg_r1
            PartDef {
                offset: Vec3::new(0.35, 2.0, 0.01),
                rotation: Vec3::new(0.0, 0.0, 0.2618),
                parent: Some(7),
                boxes: vec![ModelBox {
                    from: Vec3::new(-2.0, -2.5, -2.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [0.0, 27.0],
                }],
            },
            // 9: Left Leg (child of root)
            PartDef {
                offset: Vec3::new(1.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: Vec::new(),
            },
            // 10: Left Leg_r1
            PartDef {
                offset: Vec3::new(1.65, 2.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, -0.2618),
                parent: Some(9),
                boxes: vec![ModelBox {
                    from: Vec3::new(-2.0, -2.5, -2.0),
                    size: Vec3::new(4.0, 5.0, 4.0),
                    tex_offset: [16.0, 27.0],
                }],
            },
        ],
    }
}

fn closest_direction(normal: Vec3) -> Direction {
    let mut best_dir = Direction::Up;
    let mut best_dot = -999.0f32;
    for candidate in Direction::ALL {
        let dot = normal.dot(candidate.normal());
        if dot > best_dot {
            best_dot = dot;
            best_dir = candidate;
        }
    }
    best_dir
}

/// Computes the 6 standard faces for a local cuboid box with 64x64 texture mapping.
/// Follows Minecraft vanilla `ModelPart.Cube` and `ModelPart.Polygon` UV unwrapping.
fn make_box_faces(b: &ModelBox, model_matrix: Mat4, texture: &str) -> Vec<(Direction, BakedFace)> {
    let x = b.from.x;
    let y = b.from.y;
    let z = b.from.z;
    let dx = b.size.x;
    let dy = b.size.y;
    let dz = b.size.z;

    let [u, v] = b.tex_offset;

    // Standard Minecraft ModelPart.Cube texture bounds in 64x64 pixel space:
    // u0 = xTexOffs
    // u1 = xTexOffs + depth
    // u2 = xTexOffs + depth + width
    // u22 = xTexOffs + depth + 2 * width
    // u3 = xTexOffs + 2 * depth + width
    // u4 = xTexOffs + 2 * (depth + width)
    let u0 = u;
    let u1 = u + dz;
    let u2 = u + dz + dx;
    let u22 = u + dz + 2.0 * dx;
    let u3 = u + 2.0 * dz + dx;
    let u4 = u + 2.0 * (dz + dx);

    // v0 = yTexOffs
    // v1 = yTexOffs + depth
    // v2 = yTexOffs + depth + height
    let v0 = v;
    let v1 = v + dz;
    let v2 = v + dz + dy;

    // 8 canonical cuboid corner vertices in entity space:
    let t0 = Vec3::new(x, y, z);
    let t1 = Vec3::new(x + dx, y, z);
    let t2 = Vec3::new(x + dx, y + dy, z);
    let t3 = Vec3::new(x, y + dy, z);
    let l0 = Vec3::new(x, y, z + dz);
    let l1 = Vec3::new(x + dx, y, z + dz);
    let l2 = Vec3::new(x + dx, y + dy, z + dz);
    let l3 = Vec3::new(x, y + dy, z + dz);

    // 6 outward CCW face specifications with per-vertex UVs from vanilla ModelPart:
    let face_specs = [
        // Up (entity minY face, facing -Y in entity coords, ModelPart Direction.DOWN):
        (
            Direction::Up,
            [t0, t1, l1, l0],
            [
                Vec2::new(u1 / 64.0, v1 / 64.0),
                Vec2::new(u2 / 64.0, v1 / 64.0),
                Vec2::new(u2 / 64.0, v0 / 64.0),
                Vec2::new(u1 / 64.0, v0 / 64.0),
            ],
            [u1 / 64.0, v0 / 64.0, u2 / 64.0, v1 / 64.0],
        ),
        // Down (entity maxY face, facing +Y in entity coords, ModelPart Direction.UP):
        (
            Direction::Down,
            [l3, l2, t2, t3],
            [
                Vec2::new(u2 / 64.0, v0 / 64.0),
                Vec2::new(u22 / 64.0, v0 / 64.0),
                Vec2::new(u22 / 64.0, v1 / 64.0),
                Vec2::new(u2 / 64.0, v1 / 64.0),
            ],
            [u2 / 64.0, v0 / 64.0, u22 / 64.0, v1 / 64.0],
        ),
        // North (entity minZ face, facing -Z in entity coords, ModelPart Direction.NORTH):
        (
            Direction::North,
            [t1, t0, t3, t2],
            [
                Vec2::new(u2 / 64.0, v1 / 64.0),
                Vec2::new(u1 / 64.0, v1 / 64.0),
                Vec2::new(u1 / 64.0, v2 / 64.0),
                Vec2::new(u2 / 64.0, v2 / 64.0),
            ],
            [u1 / 64.0, v1 / 64.0, u2 / 64.0, v2 / 64.0],
        ),
        // South (entity maxZ face, facing +Z in entity coords, ModelPart Direction.SOUTH):
        (
            Direction::South,
            [l0, l1, l2, l3],
            [
                Vec2::new(u4 / 64.0, v1 / 64.0),
                Vec2::new(u3 / 64.0, v1 / 64.0),
                Vec2::new(u3 / 64.0, v2 / 64.0),
                Vec2::new(u4 / 64.0, v2 / 64.0),
            ],
            [u3 / 64.0, v1 / 64.0, u4 / 64.0, v2 / 64.0],
        ),
        // West (entity minX face, facing -X in entity coords, ModelPart Direction.WEST):
        (
            Direction::West,
            [t0, l0, l3, t3],
            [
                Vec2::new(u1 / 64.0, v1 / 64.0),
                Vec2::new(u0 / 64.0, v1 / 64.0),
                Vec2::new(u0 / 64.0, v2 / 64.0),
                Vec2::new(u1 / 64.0, v2 / 64.0),
            ],
            [u0 / 64.0, v1 / 64.0, u1 / 64.0, v2 / 64.0],
        ),
        // East (entity maxX face, facing +X in entity coords, ModelPart Direction.EAST):
        (
            Direction::East,
            [l1, t1, t2, l2],
            [
                Vec2::new(u3 / 64.0, v1 / 64.0),
                Vec2::new(u2 / 64.0, v1 / 64.0),
                Vec2::new(u2 / 64.0, v2 / 64.0),
                Vec2::new(u3 / 64.0, v2 / 64.0),
            ],
            [u2 / 64.0, v1 / 64.0, u3 / 64.0, v2 / 64.0],
        ),
    ];

    let mut faces = Vec::with_capacity(6);

    for (orig_dir, raw_pts, uvs, uv_bounds) in face_specs {
        // Transform vertices to normalized [0.0, 1.0] block space:
        let mut transformed_verts = [Vec3::ZERO; 4];
        for (i, p) in raw_pts.iter().enumerate() {
            let p_world = model_matrix.transform_point3(*p);
            // Convert from 16-unit block coordinate to [0.0, 1.0]
            transformed_verts[i] = p_world / 16.0;
        }

        let v1 = transformed_verts[1] - transformed_verts[0];
        let v2 = transformed_verts[2] - transformed_verts[0];
        let normal = v1.cross(v2).normalize_or_zero();
        let direction = closest_direction(normal);

        faces.push((
            orig_dir,
            BakedFace {
                direction,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds,
                tint_index: -1,
                cullface: None,
                vertices: transformed_verts,
                uvs,
                normal,
                atlas_uvs: None,
                atlas_chunk_id: None,
                atlas_texture_id: None,
            },
        ));
    }

    faces
}

/// Creates a fully baked, directional `BakedModel` for a Copper Golem Statue BlockState.
pub fn create_copper_statue_baked_model(blockstate: &BlockState) -> BakedModel {
    let oxidation = CopperOxidation::from_block_name(&blockstate.name);
    let texture = oxidation.texture_path();

    let pose_str = blockstate
        .properties
        .get("copper_golem_pose")
        .map(|s| s.as_str())
        .unwrap_or("standing");
    let pose = CopperStatuePose::parse(pose_str);

    let facing_str = blockstate
        .properties
        .get("facing")
        .map(|s| s.as_str())
        .unwrap_or("north");
    let facing = Direction::parse_loose(facing_str).unwrap_or(Direction::North);

    // Facing rotation in degrees around Y (matching Minecraft ModelTransformation):
    // North: 0°, South: 180°, East: 90°, West: 270° (-90°)
    let y_rot_rad = match facing {
        Direction::North => 0.0,
        Direction::East => std::f32::consts::FRAC_PI_2,
        Direction::South => std::f32::consts::PI,
        Direction::West => -std::f32::consts::FRAC_PI_2,
        _ => 0.0,
    };

    // Root block-space transformation:
    // 1. Center of block at [8.0, 0.0, 8.0]
    // 2. Rotate around Y by facing angle
    // 3. CopperGolemStatueModel setupAnim: root.zRot = PI (flips entity Y-down to block Y-up)
    let global_root = Mat4::from_translation(Vec3::new(8.0, 0.0, 8.0))
        * Mat4::from_rotation_y(y_rot_rad)
        * Mat4::from_rotation_z(std::f32::consts::PI);

    let parts = build_parts_for_pose(pose);
    let mut part_matrices = vec![Mat4::IDENTITY; parts.len()];

    for (i, part) in parts.iter().enumerate() {
        // Minecraft PoseStack order: translate(x, y, z) * rotZ(rz) * rotY(ry) * rotX(rx)
        let local_mat = Mat4::from_translation(part.offset)
            * Mat4::from_rotation_z(part.rotation.z)
            * Mat4::from_rotation_y(part.rotation.y)
            * Mat4::from_rotation_x(part.rotation.x);

        let parent_mat = match part.parent {
            Some(p_idx) => part_matrices[p_idx],
            None => global_root,
        };

        part_matrices[i] = parent_mat * local_mat;
    }

    let mut baked_elements = Vec::new();

    for (i, part) in parts.iter().enumerate() {
        let mat = part_matrices[i];
        for b in &part.boxes {
            let faces = make_box_faces(b, mat, texture);
            let mut face_map = HashMap::new();
            for (orig_dir, f) in faces {
                face_map.insert(orig_dir, f);
            }

            // Accurate axis-aligned bounding box covering all 8 rotated box corners:
            let mut min_p = Vec3::splat(f32::INFINITY);
            let mut max_p = Vec3::splat(f32::NEG_INFINITY);
            for dx in [0.0, b.size.x] {
                for dy in [0.0, b.size.y] {
                    for dz in [0.0, b.size.z] {
                        let corner = mat.transform_point3(b.from + Vec3::new(dx, dy, dz));
                        min_p = min_p.min(corner);
                        max_p = max_p.max(corner);
                    }
                }
            }

            baked_elements.push(BakedElement {
                from_pos: [min_p.x, min_p.y, min_p.z],
                to_pos: [max_p.x, max_p.y, max_p.z],
                faces: face_map,
            });
        }
    }

    let mut found_faces: [Option<BakedFace>; 6] = [None, None, None, None, None, None];
    for elem in &baked_elements {
        for face in elem.faces.values() {
            let idx = face.direction.to_index();
            if found_faces[idx].is_none() {
                found_faces[idx] = Some(face.clone());
            }
        }
    }

    let mut six_faces = [
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
        BakedFace::default(),
    ];
    for dir in Direction::ALL {
        let idx = dir.to_index();
        if let Some(f) = found_faces[idx].take() {
            six_faces[idx] = f;
        } else {
            six_faces[idx] = BakedFace {
                direction: dir,
                texture: texture.to_string(),
                uv_rot: 0.0,
                uv_bounds: [0.0, 0.0, 1.0, 1.0],
                tint_index: -1,
                cullface: None,
                vertices: [Vec3::ZERO; 4],
                uvs: [Vec2::ZERO; 4],
                normal: dir.normal(),
                ..Default::default()
            };
        }
    }

    let canonical_str = blockstate.to_canonical_string();
    let mut cull_meta = mtk_cull::compute_non_occluding_meta(&canonical_str);
    cull_meta.is_full_cube = false;
    cull_meta.is_opaque = false;

    let mut model = BakedModel {
        block_state: canonical_str,
        elements: baked_elements,
        obj_faces: Vec::new(),
        faces: six_faces,
        is_cube: false,
        is_opaque: false,
        is_emissive: false,
        cull_meta: Some(cull_meta),
        culled_faces: Default::default(),
        unculled_faces: Default::default(),
    };

    model.rebuild_face_buckets();
    model
}
