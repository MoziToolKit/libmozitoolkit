use glam::IVec3;
use mtk_voxel::{SectionStorage, VoxelError, VoxelReader, VoxelSource, VoxelStorage, VoxelWriter};

/// Mock voxel source generating procedural test sections
struct MockVoxelSource {
    sections: std::collections::HashMap<IVec3, SectionStorage>,
}

impl MockVoxelSource {
    fn new() -> Self {
        let mut sections = std::collections::HashMap::new();
        let mut sec = SectionStorage::new(IVec3::new(0, 0, 0));
        sec.set_local(0, 0, 0, "minecraft:diamond_block");
        sections.insert(IVec3::new(0, 0, 0), sec);

        let mut sec2 = SectionStorage::new(IVec3::new(1, 0, 0));
        sec2.set_local(15, 15, 15, "minecraft:gold_block");
        sections.insert(IVec3::new(1, 0, 0), sec2);

        Self { sections }
    }
}

impl VoxelSource for MockVoxelSource {
    fn source_name(&self) -> &str {
        "mock_procedural_source"
    }

    fn estimated_section_count(&self) -> Option<usize> {
        Some(self.sections.len())
    }

    fn has_section(&self, sx: i32, sy: i32, sz: i32) -> bool {
        self.sections.contains_key(&IVec3::new(sx, sy, sz))
    }

    fn load_section(
        &mut self,
        sx: i32,
        sy: i32,
        sz: i32,
    ) -> Result<Option<SectionStorage>, VoxelError> {
        Ok(self.sections.get(&IVec3::new(sx, sy, sz)).cloned())
    }

    fn section_bounds(&self) -> Option<(IVec3, IVec3)> {
        Some((IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)))
    }
}

#[test]
fn test_voxel_source_ingestion_and_traits() {
    let mut source = MockVoxelSource::new();
    let mut storage = VoxelStorage::new();

    // 1. Ingest via VoxelStorage::ingest_source
    let count = storage
        .ingest_source(&mut source, None)
        .expect("Failed to ingest source");
    assert_eq!(count, 2);

    // 2. Query via VoxelReader trait
    let reader: &dyn VoxelReader = &storage;
    assert_eq!(reader.get_block(0, 0, 0), "minecraft:diamond_block");
    assert_eq!(reader.get_block(31, 15, 15), "minecraft:gold_block");
    assert_eq!(reader.get_block(1, 1, 1), "minecraft:air");

    // 3. Mutate via VoxelWriter trait
    let writer: &mut dyn VoxelWriter = &mut storage;
    writer.set_block(0, 1, 0, "minecraft:emerald_block", None);

    let reader: &dyn VoxelReader = &storage;
    assert_eq!(reader.get_block(0, 1, 0), "minecraft:emerald_block");
}
