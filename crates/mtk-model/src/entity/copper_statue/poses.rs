use glam::Vec3;

use super::CopperStatuePose;

/// Local cuboid box definition with 64x64 texture offsets.
#[derive(Debug, Clone)]
pub(crate) struct ModelBox {
    pub(crate) from: Vec3,
    pub(crate) size: Vec3,
    pub(crate) tex_offset: [f32; 2],
}

impl ModelBox {
    #[inline]
    pub(crate) const fn new(from: Vec3, size: Vec3, tex_offset: [f32; 2]) -> Self {
        Self {
            from,
            size,
            tex_offset,
        }
    }
}

/// Hierarchical model part definition.
#[derive(Debug, Clone)]
pub(crate) struct PartDef {
    pub(crate) offset: Vec3,
    pub(crate) rotation: Vec3, // (rx, ry, rz) radians
    pub(crate) parent: Option<usize>,
    pub(crate) boxes: Vec<ModelBox>,
}

impl PartDef {
    #[inline]
    pub(crate) const fn root() -> Self {
        Self {
            offset: Vec3::ZERO,
            rotation: Vec3::ZERO,
            parent: None,
            boxes: Vec::new(),
        }
    }

    #[inline]
    pub(crate) const fn empty(offset: Vec3, parent: usize) -> Self {
        Self {
            offset,
            rotation: Vec3::ZERO,
            parent: Some(parent),
            boxes: Vec::new(),
        }
    }

    #[inline]
    pub(crate) const fn with_rotation(offset: Vec3, rotation: Vec3, parent: usize) -> Self {
        Self {
            offset,
            rotation,
            parent: Some(parent),
            boxes: Vec::new(),
        }
    }
}

/// Constructs the parts hierarchy for the given pose.
pub(crate) fn build_parts_for_pose(pose: CopperStatuePose) -> Vec<PartDef> {
    match pose {
        CopperStatuePose::Standing => vec![
            PartDef::root(),
            // 1: Body
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox::new(
                    Vec3::new(-4.0, -6.0, -3.0),
                    Vec3::new(8.0, 6.0, 6.0),
                    [0.0, 15.0],
                )],
            },
            // 2: Head (child of body)
            PartDef {
                offset: Vec3::new(0.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox::new(
                        Vec3::new(-4.0, -5.0, -5.0),
                        Vec3::new(8.0, 5.0, 10.0),
                        [0.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.0, -2.0, -6.0),
                        Vec3::new(2.0, 3.0, 2.0),
                        [56.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.0, -9.0, -1.0),
                        Vec3::new(2.0, 4.0, 2.0),
                        [37.0, 8.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-2.0, -13.0, -2.0),
                        Vec3::new(4.0, 4.0, 4.0),
                        [37.0, 0.0],
                    ),
                ],
            },
            // 3: Right Arm (child of body)
            PartDef {
                offset: Vec3::new(-4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![ModelBox::new(
                    Vec3::new(-3.0, -1.0, -2.0),
                    Vec3::new(3.0, 10.0, 4.0),
                    [36.0, 16.0],
                )],
            },
            // 4: Left Arm (child of body)
            PartDef {
                offset: Vec3::new(4.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![ModelBox::new(
                    Vec3::new(0.0, -1.0, -2.0),
                    Vec3::new(3.0, 10.0, 4.0),
                    [50.0, 16.0],
                )],
            },
            // 5: Right Leg (child of root)
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox::new(
                    Vec3::new(-4.0, 0.0, -2.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [0.0, 27.0],
                )],
            },
            // 6: Left Leg (child of root)
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox::new(
                    Vec3::new(0.0, 0.0, -2.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [16.0, 27.0],
                )],
            },
        ],

        CopperStatuePose::Running => vec![
            PartDef::root(),
            PartDef::empty(Vec3::new(-1.064, -5.0, 0.0), 0),
            // 2: Body_r1 (child of body)
            PartDef {
                offset: Vec3::new(1.1, 0.1, 0.7),
                rotation: Vec3::new(0.1204, -0.0064, -0.0779),
                parent: Some(1),
                boxes: vec![ModelBox::new(
                    Vec3::new(-4.02, -6.116, -3.5),
                    Vec3::new(8.0, 6.0, 6.0),
                    [0.0, 15.0],
                )],
            },
            // 3: Head (child of body)
            PartDef {
                offset: Vec3::new(0.7, -5.6, -1.8),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox::new(
                        Vec3::new(-4.0, -5.1, -5.0),
                        Vec3::new(8.0, 5.0, 10.0),
                        [0.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.02, -2.1, -6.0),
                        Vec3::new(2.0, 3.0, 2.0),
                        [56.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.02, -9.1, -1.0),
                        Vec3::new(2.0, 4.0, 2.0),
                        [37.0, 8.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-2.0, -13.1, -2.0),
                        Vec3::new(4.0, 4.0, 4.0),
                        [37.0, 0.0],
                    ),
                ],
            },
            PartDef::empty(Vec3::new(-4.0, -6.0, 0.0), 1),
            // 5: Right Arm_r1 (child of Right Arm)
            PartDef {
                offset: Vec3::new(0.7, -0.248, -1.62),
                rotation: Vec3::new(1.0036, 0.0, 0.0),
                parent: Some(4),
                boxes: vec![ModelBox::new(
                    Vec3::new(-3.052, -1.11, -2.036),
                    Vec3::new(3.0, 10.0, 4.0),
                    [36.0, 16.0],
                )],
            },
            PartDef::empty(Vec3::new(4.0, -6.0, 0.0), 1),
            // 7: Left Arm_r1 (child of Left Arm)
            PartDef {
                offset: Vec3::new(0.732, 0.0, 0.0),
                rotation: Vec3::new(-0.8715, -0.0535, -0.0449),
                parent: Some(6),
                boxes: vec![ModelBox::new(
                    Vec3::new(0.032, -1.1, -2.0),
                    Vec3::new(3.0, 10.0, 4.0),
                    [50.0, 16.0],
                )],
            },
            PartDef::empty(Vec3::new(-3.064, -5.0, 0.0), 0),
            // 9: Right Leg_r1 (child of Right Leg)
            PartDef {
                offset: Vec3::new(1.048, 0.0, -0.9),
                rotation: Vec3::new(-0.8727, 0.0, 0.0),
                parent: Some(8),
                boxes: vec![ModelBox::new(
                    Vec3::new(-1.856, -0.1, -1.09),
                    Vec3::new(4.0, 5.0, 4.0),
                    [0.0, 27.0],
                )],
            },
            PartDef::empty(Vec3::new(0.936, -5.0, 0.0), 0),
            // 11: Left Leg_r1 (child of Left Leg)
            PartDef {
                offset: Vec3::new(1.0, 0.0, 0.0),
                rotation: Vec3::new(std::f32::consts::FRAC_PI_4, 0.0, 0.0),
                parent: Some(10),
                boxes: vec![ModelBox::new(
                    Vec3::new(-2.088, -0.1, -2.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [16.0, 27.0],
                )],
            },
        ],

        CopperStatuePose::Sitting => vec![
            PartDef::root(),
            // 1: Body
            PartDef {
                offset: Vec3::new(0.0, -3.0, 2.325),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![
                    ModelBox::new(
                        Vec3::new(-3.0, -4.0, -4.525),
                        Vec3::new(6.0, 1.0, 6.0),
                        [3.0, 19.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-4.0, -3.0, -3.525),
                        Vec3::new(8.0, 6.0, 6.0),
                        [0.0, 15.0],
                    ),
                ],
            },
            // 2: Body_r1 (child of body)
            PartDef {
                offset: Vec3::new(0.0, -1.0, -4.325),
                rotation: Vec3::new(0.0, 0.0, -std::f32::consts::PI),
                parent: Some(1),
                boxes: vec![ModelBox::new(
                    Vec3::new(-4.0, -3.0, -2.2),
                    Vec3::new(8.0, 6.0, 3.0),
                    [3.0, 18.0],
                )],
            },
            // 3: Head (child of body)
            PartDef {
                offset: Vec3::new(0.0, -6.0, -0.2),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox::new(
                        Vec3::new(-1.0, -7.0, -3.3),
                        Vec3::new(2.0, 4.0, 2.0),
                        [37.0, 8.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-2.0, -11.0, -4.3),
                        Vec3::new(4.0, 4.0, 4.0),
                        [37.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-4.0, -3.0, -7.325),
                        Vec3::new(8.0, 5.0, 10.0),
                        [0.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.0, 0.0, -8.325),
                        Vec3::new(2.0, 3.0, 2.0),
                        [56.0, 0.0],
                    ),
                ],
            },
            PartDef::with_rotation(Vec3::new(-4.0, -5.6, -1.8), Vec3::new(0.4363, 0.0, 0.0), 1),
            // 5: Right Arm_r1
            PartDef {
                offset: Vec3::new(0.0, 0.0893, 0.1198),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_3, 0.0, 0.0),
                parent: Some(4),
                boxes: vec![ModelBox::new(
                    Vec3::new(-3.075, -0.9733, -1.9966),
                    Vec3::new(3.0, 10.0, 4.0),
                    [36.0, 16.0],
                )],
            },
            PartDef::with_rotation(Vec3::new(4.0, -5.6, -1.7), Vec3::new(0.4363, 0.0, 0.0), 1),
            // 7: Left Arm_r1
            PartDef {
                offset: Vec3::new(0.0, -0.0015, -0.0808),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_3, 0.0, 0.0),
                parent: Some(6),
                boxes: vec![ModelBox::new(
                    Vec3::new(0.075, -1.0443, -1.8997),
                    Vec3::new(3.0, 10.0, 4.0),
                    [50.0, 16.0],
                )],
            },
            PartDef::empty(Vec3::new(-2.1, -2.1, -2.075), 0),
            // 9: Right Leg_r1
            PartDef {
                offset: Vec3::new(0.05, -1.9, 1.075),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0),
                parent: Some(8),
                boxes: vec![ModelBox::new(
                    Vec3::new(-2.0, 0.975, 0.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [0.0, 27.0],
                )],
            },
            PartDef::empty(Vec3::new(2.0, -2.0, -2.075), 0),
            // 11: Left Leg_r1
            PartDef {
                offset: Vec3::new(0.05, -2.0, 1.075),
                rotation: Vec3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0),
                parent: Some(10),
                boxes: vec![ModelBox::new(
                    Vec3::new(-2.0, 0.975, 0.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [16.0, 27.0],
                )],
            },
        ],

        CopperStatuePose::Star => vec![
            PartDef::root(),
            // 1: Body
            PartDef {
                offset: Vec3::new(0.0, -5.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(0),
                boxes: vec![ModelBox::new(
                    Vec3::new(-4.0, -6.0, -3.0),
                    Vec3::new(8.0, 6.0, 6.0),
                    [0.0, 15.0],
                )],
            },
            // 2: Head (child of body)
            PartDef {
                offset: Vec3::new(0.0, -6.0, 0.0),
                rotation: Vec3::ZERO,
                parent: Some(1),
                boxes: vec![
                    ModelBox::new(
                        Vec3::new(-4.0, -5.0, -5.0),
                        Vec3::new(8.0, 5.0, 10.0),
                        [0.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.0, -2.0, -6.0),
                        Vec3::new(2.0, 3.0, 2.0),
                        [56.0, 0.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-1.0, -9.0, -1.0),
                        Vec3::new(2.0, 4.0, 2.0),
                        [37.0, 8.0],
                    ),
                    ModelBox::new(
                        Vec3::new(-2.0, -13.0, -2.0),
                        Vec3::new(4.0, 4.0, 4.0),
                        [37.0, 0.0],
                    ),
                ],
            },
            PartDef::empty(Vec3::new(-4.0, -6.0, 0.0), 1),
            // 4: Right Arm_r1
            PartDef {
                offset: Vec3::new(1.0, 1.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, 1.9199),
                parent: Some(3),
                boxes: vec![ModelBox::new(
                    Vec3::new(-1.5, -5.0, -2.0),
                    Vec3::new(3.0, 10.0, 4.0),
                    [36.0, 16.0],
                )],
            },
            PartDef::empty(Vec3::new(4.0, -6.0, 0.0), 1),
            // 6: Left Arm_r1
            PartDef {
                offset: Vec3::new(-1.0, 1.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, -1.9199),
                parent: Some(5),
                boxes: vec![ModelBox::new(
                    Vec3::new(-1.5, -5.0, -2.0),
                    Vec3::new(3.0, 10.0, 4.0),
                    [50.0, 16.0],
                )],
            },
            PartDef::empty(Vec3::new(-3.0, -5.0, 0.0), 0),
            // 8: Right Leg_r1
            PartDef {
                offset: Vec3::new(0.35, 2.0, 0.01),
                rotation: Vec3::new(0.0, 0.0, 0.2618),
                parent: Some(7),
                boxes: vec![ModelBox::new(
                    Vec3::new(-2.0, -2.5, -2.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [0.0, 27.0],
                )],
            },
            PartDef::empty(Vec3::new(1.0, -5.0, 0.0), 0),
            // 10: Left Leg_r1
            PartDef {
                offset: Vec3::new(1.65, 2.0, 0.0),
                rotation: Vec3::new(0.0, 0.0, -0.2618),
                parent: Some(9),
                boxes: vec![ModelBox::new(
                    Vec3::new(-2.0, -2.5, -2.0),
                    Vec3::new(4.0, 5.0, 4.0),
                    [16.0, 27.0],
                )],
            },
        ],
    }
}
