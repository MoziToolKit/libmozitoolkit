pub mod algorithms;
pub mod parser;
pub mod solver;
pub mod tables;
pub mod types;

pub use parser::parse_ctm_properties;
pub use solver::CtmSolver;
pub use tables::{
    build_ctm_47_lookup, build_overlay_17_lookup, CTM_47_LOOKUP, OVERLAY_17_LOOKUP,
    TILE_TO_CONNECTION_DATA, TILE_TO_OVERLAY_DATA,
};
pub use types::{
    coordinate_random, extract_block_name, get_face_tangents, BlockMatch, ConnectLogic, CtmMethod,
    CtmRule, CtmSymmetry,
};

impl CtmRule {
    /// Parse an OptiFine / Continuity `.properties` file into a `CtmRule`.
    #[inline]
    pub fn parse_properties(source_path: &str, namespace: &str, content: &str) -> Option<Self> {
        parse_ctm_properties(source_path, namespace, content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ctm_47_lookup_table() {
        assert_eq!(CTM_47_LOOKUP[0b00000000], 0);
        assert_eq!(CTM_47_LOOKUP[0b11111111], 26);
        assert_eq!(CTM_47_LOOKUP[0b01010101], 46);
        // Corner degeneration: top-left corner isolated without top or left -> should map to 0
        assert_eq!(CTM_47_LOOKUP[0b10000000], 0);
    }

    #[test]
    fn test_ctm_horizontal_and_vertical() {
        let content_h = r#"
matchBlocks=minecraft:bookshelf
method=horizontal
tiles=0 1 2 3
"#;
        let rule_h =
            CtmRule::parse_properties("optifine/ctm/bookshelf.properties", "minecraft", content_h)
                .unwrap();
        let solver = CtmSolver::new(vec![rule_h]);

        // Query bookshelf face
        use glam::IVec3;
        use mtk_core::Direction;
        let pos = IVec3::new(0, 0, 0);

        // North face: looking in +Z direction.
        // East (+X) is to the LEFT.
        // West (-X) is to the RIGHT.

        // 1. Neighbor at (1, 0, 0) (East = left) -> left connected -> tile 2
        let get_left = |p: IVec3| {
            if p == IVec3::new(1, 0, 0) {
                Some("minecraft:bookshelf")
            } else {
                None
            }
        };
        let tile_left = solver.resolve_face(
            "minecraft:bookshelf",
            Direction::North,
            pos,
            None,
            None,
            get_left,
        );
        assert_eq!(tile_left.unwrap().path, "optifine/ctm/2");

        // 2. Neighbor at (-1, 0, 0) (West = right) -> right connected -> tile 0
        let get_right = |p: IVec3| {
            if p == IVec3::new(-1, 0, 0) {
                Some("minecraft:bookshelf")
            } else {
                None
            }
        };
        let tile_right = solver.resolve_face(
            "minecraft:bookshelf",
            Direction::North,
            pos,
            None,
            None,
            get_right,
        );
        assert_eq!(tile_right.unwrap().path, "optifine/ctm/0");
    }

    #[test]
    fn test_ctm_repeat_method() {
        let content_repeat = r#"
matchBlocks=sandstone
method=repeat
width=2
height=2
tiles=0 1 2 3
"#;
        let rule_repeat = CtmRule::parse_properties(
            "optifine/ctm/sandstone.properties",
            "minecraft",
            content_repeat,
        )
        .unwrap();
        let solver = CtmSolver::new(vec![rule_repeat]);

        use glam::IVec3;
        use mtk_core::Direction;
        let dummy = |_: IVec3| None::<&str>;

        // (0, 0, 0) -> u=0, v=0 -> tile 0
        let t0 = solver
            .resolve_face(
                "minecraft:sandstone",
                Direction::North,
                IVec3::new(0, 0, 0),
                None,
                None,
                dummy,
            )
            .unwrap();
        assert_eq!(t0.path, "optifine/ctm/0");

        // (-1, 0, 0) -> Right (West) -> u=1, v=0 -> tile 1
        let t1 = solver
            .resolve_face(
                "minecraft:sandstone",
                Direction::North,
                IVec3::new(-1, 0, 0),
                None,
                None,
                dummy,
            )
            .unwrap();
        assert_eq!(t1.path, "optifine/ctm/1");

        // (0, -1, 0) -> Down -> u=0, v=1 -> tile 2
        let t2 = solver
            .resolve_face(
                "minecraft:sandstone",
                Direction::North,
                IVec3::new(0, -1, 0),
                None,
                None,
                dummy,
            )
            .unwrap();
        assert_eq!(t2.path, "optifine/ctm/2");

        // (-1, -1, 0) -> Right + Down -> u=1, v=1 -> tile 3
        let t3 = solver
            .resolve_face(
                "minecraft:sandstone",
                Direction::North,
                IVec3::new(-1, -1, 0),
                None,
                None,
                dummy,
            )
            .unwrap();
        assert_eq!(t3.path, "optifine/ctm/3");
    }

    #[test]
    fn test_ctm_custom_mod_namespace() {
        let content = r#"
matchBlocks=create:cogwheel
method=horizontal
tiles=0 1 2 3
"#;
        let rule = CtmRule::parse_properties(
            "assets/create/optifine/ctm/cogwheel.properties",
            "create",
            content,
        )
        .unwrap();
        assert_eq!(rule.match_blocks[0].block, "create:cogwheel");
        assert_eq!(rule.tiles[0].as_ref().unwrap().namespace, "create");
        assert_eq!(
            rule.tiles[0].as_ref().unwrap().path,
            "assets/create/optifine/ctm/0"
        );

        let solver = CtmSolver::new(vec![rule]);
        let pos = glam::IVec3::new(0, 0, 0);
        // Neighbor at (-1, 0, 0) (West = right of North face) -> right connected -> tile 0
        let get_block = |p: glam::IVec3| {
            if p == glam::IVec3::new(-1, 0, 0) {
                Some("create:cogwheel")
            } else {
                None
            }
        };
        let tile = solver.resolve_face(
            "create:cogwheel",
            mtk_core::Direction::North,
            pos,
            None,
            None,
            get_block,
        );
        assert_eq!(
            tile.unwrap().to_string(),
            "create:assets/create/optifine/ctm/0"
        );
    }

    #[test]
    fn test_full_47_ctm_glass_all_directions() {
        let tiles_str = (0..=46)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let content = format!(
            "matchBlocks=glass\nmethod=ctm\ntiles={}\nconnect=block\n",
            tiles_str
        );
        let rule = CtmRule::parse_properties(
            "optifine/ctm/default/glass.properties",
            "minecraft",
            &content,
        )
        .unwrap();
        let solver = CtmSolver::new(vec![rule]);

        use glam::IVec3;
        use mtk_core::Direction;
        let pos = IVec3::new(0, 0, 0);

        // Helper to resolve face with a single neighbor
        let check = |face: Direction, neighbor_offset: IVec3| -> usize {
            let get_b = |p: IVec3| {
                if p == pos + neighbor_offset {
                    Some("minecraft:glass")
                } else {
                    None
                }
            };
            let tile = solver
                .resolve_face("minecraft:glass", face, pos, None, None, get_b)
                .unwrap();
            tile.path
                .strip_prefix("optifine/ctm/default/")
                .unwrap()
                .parse::<usize>()
                .unwrap()
        };

        // No connections -> Tile 0
        let no_neighbor = |_: IVec3| None::<&str>;
        let t0 = solver
            .resolve_face(
                "minecraft:glass",
                Direction::North,
                pos,
                None,
                None,
                no_neighbor,
            )
            .unwrap();
        assert_eq!(t0.path, "optifine/ctm/default/0");

        // NORTH face: left is East (+X), right is West (-X), up is Up (+Y), down is Down (-Y)
        assert_eq!(check(Direction::North, IVec3::new(1, 0, 0)), 3); // Left connected -> Tile 3
        assert_eq!(check(Direction::North, IVec3::new(-1, 0, 0)), 1); // Right connected -> Tile 1
        assert_eq!(check(Direction::North, IVec3::new(0, 1, 0)), 36); // Up connected -> Tile 36
        assert_eq!(check(Direction::North, IVec3::new(0, -1, 0)), 12); // Down connected -> Tile 12

        // SOUTH face: left is West (-X), right is East (+X)
        assert_eq!(check(Direction::South, IVec3::new(-1, 0, 0)), 3); // Left connected -> Tile 3
        assert_eq!(check(Direction::South, IVec3::new(1, 0, 0)), 1); // Right connected -> Tile 1
        assert_eq!(check(Direction::South, IVec3::new(0, 1, 0)), 36); // Up connected -> Tile 36
        assert_eq!(check(Direction::South, IVec3::new(0, -1, 0)), 12); // Down connected -> Tile 12

        // EAST face: left is South (+Z), right is North (-Z)
        assert_eq!(check(Direction::East, IVec3::new(0, 0, 1)), 3); // Left connected -> Tile 3
        assert_eq!(check(Direction::East, IVec3::new(0, 0, -1)), 1); // Right connected -> Tile 1
        assert_eq!(check(Direction::East, IVec3::new(0, 1, 0)), 36); // Up connected -> Tile 36
        assert_eq!(check(Direction::East, IVec3::new(0, -1, 0)), 12); // Down connected -> Tile 12

        // WEST face: left is North (-Z), right is South (+Z)
        assert_eq!(check(Direction::West, IVec3::new(0, 0, -1)), 3); // Left connected -> Tile 3
        assert_eq!(check(Direction::West, IVec3::new(0, 0, 1)), 1); // Right connected -> Tile 1
        assert_eq!(check(Direction::West, IVec3::new(0, 1, 0)), 36); // Up connected -> Tile 36
        assert_eq!(check(Direction::West, IVec3::new(0, -1, 0)), 12); // Down connected -> Tile 12

        // UP face: left is West (-X), right is East (+X), up is North (-Z), down is South (+Z)
        assert_eq!(check(Direction::Up, IVec3::new(-1, 0, 0)), 3); // Left connected -> Tile 3
        assert_eq!(check(Direction::Up, IVec3::new(1, 0, 0)), 1); // Right connected -> Tile 1
        assert_eq!(check(Direction::Up, IVec3::new(0, 0, -1)), 36); // Up connected -> Tile 36
        assert_eq!(check(Direction::Up, IVec3::new(0, 0, 1)), 12); // Down connected -> Tile 12

        // DOWN face: left is West (-X), right is East (+X), up is South (+Z), down is North (-Z)
        assert_eq!(check(Direction::Down, IVec3::new(-1, 0, 0)), 3); // Left connected -> Tile 3
        assert_eq!(check(Direction::Down, IVec3::new(1, 0, 0)), 1); // Right connected -> Tile 1
        assert_eq!(check(Direction::Down, IVec3::new(0, 0, 1)), 36); // Up connected -> Tile 36
        assert_eq!(check(Direction::Down, IVec3::new(0, 0, -1)), 12); // Down connected -> Tile 12
    }
}
