use glam::IVec3;

use super::container::VoxelStorage;

impl VoxelStorage {
    /// Exports manifest metadata (bounds and empty sections) as JSON for scene persistence.
    #[cfg(feature = "serde")]
    pub fn export_manifest_metadata(&self) -> serde_json::Value {
        let empty_list: Vec<String> = self
            .known_empty_sections
            .iter()
            .map(|c| format!("{},{},{}", c.x, c.y, c.z))
            .collect();

        serde_json::json!({
            "min_x": self.min_x,
            "min_y": self.min_y,
            "min_z": self.min_z,
            "size_x": self.size_x,
            "size_y": self.size_y,
            "size_z": self.size_z,
            "generation": self.get_generation(),
            "known_empty_sections": empty_list,
        })
    }

    /// Imports manifest metadata from scene JSON properties.
    #[cfg(feature = "serde")]
    pub fn import_manifest_metadata(&mut self, data: &serde_json::Value) -> bool {
        let obj = match data.as_object() {
            Some(o) => o,
            None => return false,
        };

        self.min_x = obj.get("min_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        self.min_y = obj.get("min_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        self.min_z = obj.get("min_z").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        self.size_x = obj.get("size_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        self.size_y = obj.get("size_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        self.size_z = obj.get("size_z").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
        let gen = obj.get("generation").and_then(|v| v.as_u64()).unwrap_or(0);
        self.set_generation(gen);

        self.known_empty_sections.clear();
        if let Some(empty_arr) = obj.get("known_empty_sections").and_then(|v| v.as_array()) {
            for v in empty_arr {
                if let Some(s) = v.as_str() {
                    let parts: Vec<&str> = s.split(',').collect();
                    if parts.len() == 3 {
                        if let (Ok(x), Ok(y), Ok(z)) = (
                            parts[0].parse::<i32>(),
                            parts[1].parse::<i32>(),
                            parts[2].parse::<i32>(),
                        ) {
                            self.known_empty_sections.insert(IVec3::new(x, y, z));
                        }
                    }
                }
            }
        }

        true
    }

    /// Ingests sections from an external `VoxelSource` provider.
    pub fn ingest_source<S: crate::source::VoxelSource + ?Sized>(
        &mut self,
        source: &mut S,
        sections: Option<&[IVec3]>,
    ) -> Result<usize, crate::types::VoxelError> {
        crate::source::ingest_from_source(source, self, sections)
    }
}
