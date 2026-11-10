//! Procedural Banner Baker with multi-layer tinted alpha compositing and caching.

use std::collections::HashMap;
use std::sync::Arc;

use mtk_resource::banner::{dye_color_to_rgb, normalize_banner_pattern as normalize_pattern_stem};

use crate::error::TextureError;
use crate::{DynamicAtlasChunk, RgbaBuffer};

/// Headless procedural banner compositing engine.
///
/// Bakes neutral base cloth modulated by base dye color, then sequentially
/// composites pattern layers using tinted Porter-Duff alpha blending.
/// Identical banner configurations are deduplicated and cached in-memory.
#[derive(Debug, Clone, PartialEq)]
pub struct BannerBaker {
    base_cloth: RgbaBuffer,
    pattern_sprites: HashMap<String, RgbaBuffer>,
    cache: HashMap<String, Arc<RgbaBuffer>>,
}

impl Default for BannerBaker {
    fn default() -> Self {
        Self::new(None)
    }
}

impl BannerBaker {
    /// Creates a new `BannerBaker` with an optional custom 64x64 cloth base buffer.
    /// If `None`, initializes with synthetic neutral cloth.
    pub fn new(base_cloth: Option<RgbaBuffer>) -> Self {
        Self {
            base_cloth: base_cloth.unwrap_or_else(DynamicAtlasChunk::create_fallback_banner_base),
            pattern_sprites: HashMap::new(),
            cache: HashMap::new(),
        }
    }

    /// Registers a pattern sprite by name (e.g. "stripe_top", "flower", "creeper").
    pub fn register_pattern_sprite(&mut self, name: &str, sprite: RgbaBuffer) {
        let norm = normalize_pattern_stem(name);
        self.pattern_sprites.insert(norm.to_string(), sprite);
    }

    /// Decodes PNG bytes and registers the resulting sprite.
    pub fn register_pattern_png_bytes(
        &mut self,
        name: &str,
        png_bytes: &[u8],
    ) -> Result<(), TextureError> {
        let img = image::load_from_memory(png_bytes).map_err(TextureError::Image)?;
        let rgba = img.to_rgba8();
        let buf = RgbaBuffer {
            width: rgba.width(),
            height: rgba.height(),
            pixels: rgba.into_raw(),
        };
        self.register_pattern_sprite(name, buf);
        Ok(())
    }

    /// Returns reference to current base cloth buffer.
    pub fn base_cloth(&self) -> &RgbaBuffer {
        &self.base_cloth
    }

    /// Encodes base cloth buffer to PNG bytes.
    pub fn base_cloth_png_bytes(&self) -> Result<Vec<u8>, TextureError> {
        self.base_cloth.to_png_bytes()
    }

    /// Returns a map of pattern name to encoded PNG bytes.
    pub fn all_pattern_png_bytes(&self) -> HashMap<String, Vec<u8>> {
        let mut map = HashMap::new();
        for (name, buf) in &self.pattern_sprites {
            if let Ok(bytes) = buf.to_png_bytes() {
                map.insert(name.clone(), bytes);
            }
        }
        map
    }

    /// Loads base cloth and pattern sprites from in-memory byte representations.
    pub fn load_from_bytes(
        &mut self,
        base_png: Option<&[u8]>,
        patterns: &HashMap<String, Vec<u8>>,
    ) -> Result<usize, TextureError> {
        let mut count = 0;
        if let Some(bytes) = base_png {
            let img = image::load_from_memory(bytes).map_err(TextureError::Image)?;
            let rgba = img.to_rgba8();
            self.base_cloth = RgbaBuffer {
                width: rgba.width(),
                height: rgba.height(),
                pixels: rgba.into_raw(),
            };
            count += 1;
        }
        for (name, bytes) in patterns {
            if self.register_pattern_png_bytes(name, bytes).is_ok() {
                count += 1;
            }
        }
        self.cache.clear();
        Ok(count)
    }

    /// Scans a resource pack stack for `assets/minecraft/textures/entity/banner/*.png`
    /// and `assets/minecraft/textures/entity/banner_patterns/*.png`
    /// and populates base cloth and pattern sprites automatically.
    pub fn load_from_pack_stack(
        &mut self,
        stack: &mtk_resource::ResourcePackStack,
    ) -> Result<usize, TextureError> {
        let mut loaded_count = 0;

        // Try base cloth
        for path in &[
            "assets/minecraft/textures/entity/banner/base.png",
            "textures/entity/banner/base.png",
        ] {
            if let Some(bytes) = stack.open_asset_raw(path) {
                if let Ok(img) = image::load_from_memory(&bytes) {
                    let rgba = img.to_rgba8();
                    self.base_cloth = RgbaBuffer {
                        width: rgba.width(),
                        height: rgba.height(),
                        pixels: rgba.into_raw(),
                    };
                    loaded_count += 1;
                    break;
                }
            }
        }

        // List files in packs
        for pack in stack.packs() {
            for file in pack.list_files("assets/") {
                let is_banner_path = file.contains("textures/entity/banner/")
                    || file.contains("textures/entity/banner_patterns/");
                if is_banner_path && file.ends_with(".png") {
                    let stem = file
                        .split('/')
                        .next_back()
                        .and_then(|s| s.strip_suffix(".png"))
                        .unwrap_or_default();
                    if stem == "base" || stem == "banner_base" {
                        continue;
                    }
                    if let Some(bytes) = pack.open(&file) {
                        if self.register_pattern_png_bytes(stem, &bytes).is_ok() {
                            loaded_count += 1;
                        }
                    }
                }
            }
        }

        Ok(loaded_count)
    }

    /// Number of distinct banner designs currently cached in memory.
    pub fn cache_len(&self) -> usize {
        self.cache.len()
    }

    /// Clears the in-memory composition cache.
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }

    /// Computes the canonical fingerprint key for a banner design.
    pub fn canonical_fingerprint(
        base_color: &str,
        patterns: &[(impl AsRef<str>, impl AsRef<str>)],
    ) -> String {
        let clean_base = base_color.strip_prefix("minecraft:").unwrap_or(base_color);
        let stem_base = clean_base.strip_suffix("_dye").unwrap_or(clean_base);
        let stem_base = stem_base.strip_suffix("_wall_banner").unwrap_or(stem_base);
        let stem_base = stem_base.strip_suffix("_banner").unwrap_or(stem_base);

        let mut key = format!("banner:{}", stem_base);
        for (pat, col) in patterns {
            let norm_p = normalize_pattern_stem(pat.as_ref());
            let c = col
                .as_ref()
                .strip_prefix("minecraft:")
                .unwrap_or(col.as_ref());
            let c = c.strip_suffix("_dye").unwrap_or(c);
            key.push('+');
            key.push_str(norm_p);
            key.push('#');
            key.push_str(c);
        }
        key
    }

    /// Bakes or retrieves the 64x64 RGBA composite buffer for a banner design.
    pub fn bake(
        &mut self,
        base_color: &str,
        patterns: &[(impl AsRef<str>, impl AsRef<str>)],
    ) -> Arc<RgbaBuffer> {
        let key = Self::canonical_fingerprint(base_color, patterns);

        if let Some(cached) = self.cache.get(&key) {
            return Arc::clone(cached);
        }

        let clean_base = base_color.strip_prefix("minecraft:").unwrap_or(base_color);
        let stem_base = clean_base.strip_suffix("_dye").unwrap_or(clean_base);
        let stem_base = stem_base.strip_suffix("_wall_banner").unwrap_or(stem_base);
        let stem_base = stem_base.strip_suffix("_banner").unwrap_or(stem_base);

        let base_rgb = dye_color_to_rgb(stem_base);
        let mut cloth = self.base_cloth.to_tinted(base_rgb);

        for (pat, col) in patterns {
            let norm_p = normalize_pattern_stem(pat.as_ref());
            if let Some(pattern_buf) = self.pattern_sprites.get(norm_p) {
                let tint_rgb = dye_color_to_rgb(col.as_ref());
                cloth.blend_over_tinted(pattern_buf, tint_rgb);
            }
        }

        let baked_arc = Arc::new(cloth);
        self.cache.insert(key, Arc::clone(&baked_arc));
        baked_arc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_banner_baker_plain_banner() {
        let mut baker = BannerBaker::default();
        let red_cloth = baker.bake("red", &Vec::<(&str, &str)>::new());

        assert_eq!(red_cloth.width, 64);
        assert_eq!(red_cloth.height, 64);

        // At (10, 10), should be tinted red (approx [176, 46, 38, 255])
        let p = red_cloth.get_pixel(10, 10);
        assert!(p[0] > 150);
        assert!(p[1] < 60);
        assert!(p[2] < 50);
        assert_eq!(p[3], 255);
        assert_eq!(baker.cache_len(), 1);

        // Calling again returns cached Arc
        let red_cloth_cached = baker.bake("red", &Vec::<(&str, &str)>::new());
        assert!(Arc::ptr_eq(&red_cloth, &red_cloth_cached));
    }

    #[test]
    fn test_banner_baker_with_pattern_layers() {
        let mut baker = BannerBaker::default();

        // Register a dummy 64x64 pattern sprite with top 10 rows filled white
        let mut top_stripe = RgbaBuffer::new(64, 64);
        for y in 0..10 {
            for x in 0..42 {
                top_stripe.set_pixel(x, y, [255, 255, 255, 255]);
            }
        }
        baker.register_pattern_sprite("stripe_top", top_stripe);

        // Bake white banner with blue stripe_top
        let banner = baker.bake("white", &[("stripe_top", "blue")]);

        // (10, 5) is in the stripe_top area -> blue ([60, 68, 170])
        let p_top = banner.get_pixel(10, 5);
        assert_eq!(p_top[0], 60);
        assert_eq!(p_top[1], 68);
        assert_eq!(p_top[2], 170);
        assert_eq!(p_top[3], 255);

        // (10, 20) is below stripe_top -> white cloth
        let p_mid = banner.get_pixel(10, 20);
        assert!(p_mid[0] > 200);
        assert!(p_mid[1] > 200);
        assert!(p_mid[2] > 200);
    }
}
