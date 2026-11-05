//! Abstract asset and atlas sources for scene package building.

use std::collections::HashMap;

use mtk_texture::BannerBaker;
use mtk_texture::RgbaBuffer;

use crate::MtkError;

/// Source providing rasterized texture layers (Albedo, Normal, ORM, AUX, Overlay) for atlas chunks.
pub trait AtlasLayerSource {
    /// Loads the Albedo RGBA image for the specified atlas chunk stem (e.g. `"blocks_chunk_001"`).
    fn get_albedo(
        &mut self,
        chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError>;

    /// Loads the Normal RGBA image for the specified atlas chunk stem (if present).
    fn get_normal(
        &mut self,
        chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError>;

    /// Loads the ORM RGBA image for the specified atlas chunk stem (if present).
    fn get_orm(&mut self, chunk_id: u16, chunk_stem: &str) -> Result<Option<RgbaBuffer>, MtkError>;

    /// Loads the AUX RGBA image for the specified atlas chunk stem (if present).
    fn get_aux(&mut self, chunk_id: u16, chunk_stem: &str) -> Result<Option<RgbaBuffer>, MtkError>;

    /// Loads the Overlay RGBA image for the specified atlas chunk stem (if present).
    fn get_overlay(
        &mut self,
        chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError>;
}

/// Source providing resident colormap PNG buffers and BiomeResolver mapping JSON.
pub trait BiomeAssetSource {
    /// Loads a biome colormap PNG buffer (e.g. `"grass"`, `"foliage"`, `"dry_foliage"`).
    fn get_colormap(&mut self, name: &str) -> Result<Option<Vec<u8>>, MtkError>;

    /// Loads the authoritative BiomeResolver JSON if present in the cache.
    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError>;
}

/// Source providing CTM (Connected Textures Mod) rules.
pub trait CtmAssetSource {
    /// Loads cached CTM rules JSON if present in the cache or source.
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError>;
}

/// Source providing procedural entity baking assets (e.g. banner base and patterns).
pub trait EntityAssetSource {
    /// Loads a pre-populated `BannerBaker` containing cached base cloth and pattern sprites.
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError>;
}

/// Capabilities exposed by a `SceneAtlasSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SceneSourceCapabilities {
    pub has_colormaps: bool,
    pub has_biome_resolver: bool,
    pub has_ctm_rules: bool,
    pub has_banner_baker: bool,
}

/// Composite trait bundling all asset sources required for scene package export.
pub trait SceneAtlasSource:
    AtlasLayerSource + BiomeAssetSource + CtmAssetSource + EntityAssetSource
{
    /// Returns the active capabilities supported by this asset source.
    fn capabilities(&self) -> SceneSourceCapabilities;
}

impl AtlasLayerSource for crate::prebake::AssetCacheReader {
    fn get_albedo(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_normal(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_n.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_orm(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_orm.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_aux(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_aux.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }

    fn get_overlay(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let chunk_path = format!("atlas/textures/{}_overlay.png", chunk_stem);
        if self.package_reader().has_chunk(&chunk_path) {
            let buf = self.read_texture_rgba(&chunk_path)?;
            Ok(Some(buf))
        } else {
            Ok(None)
        }
    }
}

impl BiomeAssetSource for crate::prebake::AssetCacheReader {
    fn get_colormap(&mut self, name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        let chunk_path = format!("biome/colormap/{name}");
        if self.package_reader().has_chunk(&chunk_path) {
            let bytes = self.package_reader().read_chunk_decompressed(&chunk_path)?;
            Ok(Some(bytes))
        } else {
            Ok(None)
        }
    }

    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError> {
        if self.package_reader().has_chunk("biome/mapping") {
            let json = self.package_reader().read_chunk_str("biome/mapping")?;
            Ok(Some(json))
        } else if self.package_reader().has_chunk("biome/resolver") {
            let json = self.package_reader().read_chunk_str("biome/resolver")?;
            Ok(Some(json))
        } else {
            Ok(None)
        }
    }
}

impl CtmAssetSource for crate::prebake::AssetCacheReader {
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError> {
        if self.package_reader().has_chunk("ctm/rules") {
            let json = self.package_reader().read_chunk_str("ctm/rules")?;
            Ok(Some(json))
        } else {
            Ok(None)
        }
    }
}

impl EntityAssetSource for crate::prebake::AssetCacheReader {
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError> {
        self.load_banner_baker().map(Some)
    }
}

impl SceneAtlasSource for crate::prebake::AssetCacheReader {
    fn capabilities(&self) -> SceneSourceCapabilities {
        SceneSourceCapabilities {
            has_colormaps: self.package_reader().has_chunk("biome/colormap/grass")
                || self.package_reader().has_chunk("biome/colormap/foliage")
                || self
                    .package_reader()
                    .has_chunk("biome/colormap/dry_foliage"),
            has_biome_resolver: self.package_reader().has_chunk("biome/mapping")
                || self.package_reader().has_chunk("biome/resolver"),
            has_ctm_rules: self.package_reader().has_chunk("ctm/rules"),
            has_banner_baker: self.package_reader().has_chunk("banner/base")
                || self.package_reader().has_chunk("banner/patterns"),
        }
    }
}

impl AtlasLayerSource for HashMap<String, RgbaBuffer> {
    fn get_albedo(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_normal(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_n.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_orm(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_orm.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_aux(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_aux.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }

    fn get_overlay(
        &mut self,
        _chunk_id: u16,
        chunk_stem: &str,
    ) -> Result<Option<RgbaBuffer>, MtkError> {
        let key = format!("{}_overlay.png", chunk_stem);
        Ok(self.get(&key).cloned())
    }
}

impl BiomeAssetSource for HashMap<String, RgbaBuffer> {
    fn get_colormap(&mut self, _name: &str) -> Result<Option<Vec<u8>>, MtkError> {
        Ok(None)
    }

    fn get_biome_resolver_json(&mut self) -> Result<Option<String>, MtkError> {
        Ok(None)
    }
}

impl CtmAssetSource for HashMap<String, RgbaBuffer> {
    fn get_ctm_rules_json(&mut self) -> Result<Option<String>, MtkError> {
        Ok(None)
    }
}

impl EntityAssetSource for HashMap<String, RgbaBuffer> {
    fn get_banner_baker(&mut self) -> Result<Option<BannerBaker>, MtkError> {
        Ok(None)
    }
}

impl SceneAtlasSource for HashMap<String, RgbaBuffer> {
    fn capabilities(&self) -> SceneSourceCapabilities {
        SceneSourceCapabilities::default()
    }
}
