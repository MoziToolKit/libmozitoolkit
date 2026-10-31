//! Minecraft player skin resolution, multi-tier caching, and prebaked entity fallback system.
//!
//! Handles official Mojang CDNs, custom CDNs (e.g. LittleSkin, Ely.by),
//! negative caching for 404/invalid URLs, non-blocking disk/memory checks,
//! background HTTP fetching, and 0-second indexed local cache hits.

pub mod circuit;
pub mod index;
#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
pub mod mirror;
pub mod mod_api;
pub mod resolver;

pub use circuit::SkinCircuitBreaker;
pub use index::{
    clean_player_name, clean_uuid, compute_sha256_hex, current_unix_timestamp,
    is_mirror_fallback_hash, is_mojang_online_uuid, SkinIndex, SkinRecord, CRAFTHEAD_ALEX_HASH,
    CRAFTHEAD_STEVE_HASH, MINOTAR_FALLBACK_HASH,
};
pub use mod_api::{
    default_player_skin_texture, derive_skin_key, VANILLA_ALEX_TEXTURE, VANILLA_STEVE_TEXTURE,
};
pub use resolver::{
    decode_base64, extract_skin_url_from_property_base64, is_valid_png_header, SkinResolver,
};

#[cfg(test)]
mod tests {
    use super::*;
    use mtk_testkit::require_online;
    use std::fs;

    #[test]
    fn test_derive_skin_key() {
        // Mojang hash
        let hash = "5813943ca53225ee0ee814321cba1e420ea21f7c55f2998f9306cfad6655f69e";
        let url = format!("http://textures.minecraft.net/texture/{}", hash);
        assert_eq!(derive_skin_key(Some(&url), None, None), hash);

        // Custom CDN
        let custom_url = "https://littleskin.cn/textures/custom_user_123.png?v=1";
        let cdn_key = derive_skin_key(Some(custom_url), None, None);
        assert!(cdn_key.starts_with("cdn_"));

        // UUID fallback (with and without uuid_ prefix)
        assert_eq!(
            derive_skin_key(None, Some("4382c7a9-11f7-41c4-81fd-1592971e9eac"), None),
            "uuid_4382c7a911f741c481fd1592971e9eac"
        );
        assert_eq!(
            derive_skin_key(
                None,
                Some("uuid_4382c7a9-11f7-41c4-81fd-1592971e9eac"),
                None
            ),
            "uuid_4382c7a911f741c481fd1592971e9eac"
        );

        // Player name fallback (with and without player_ prefix)
        assert_eq!(
            derive_skin_key(None, None, Some("JaxLocke")),
            "player_jaxlocke"
        );
        assert_eq!(
            derive_skin_key(None, None, Some("player_JaxLocke")),
            "player_jaxlocke"
        );
    }

    #[test]
    fn test_default_player_skin_texture() {
        assert_eq!(
            default_player_skin_texture(None),
            "minecraft:entity/player/wide/steve"
        );
        let path = default_player_skin_texture(Some("00000000-0000-0000-0000-000000000001"));
        assert!(path == VANILLA_STEVE_TEXTURE || path == VANILLA_ALEX_TEXTURE);
    }

    #[test]
    fn test_memory_cache_registration() {
        let resolver = SkinResolver::new(None);
        let fake_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x01];
        let key = derive_skin_key(None, None, Some("custom_key_1"));
        resolver.register_memory_skin(&key, fake_png.clone());
        let fetched = resolver.try_resolve_cached(None, None, Some("custom_key_1"));
        assert_eq!(fetched, Some(fake_png));
    }

    #[test]
    fn test_negative_cache_and_resolution() {
        let resolver = SkinResolver::new(None).with_negative_cache_ttl(1);
        let invalid_url = "http://127.0.0.1:59996/invalid_skin.png";
        let png = resolver.resolve_skin_png(Some(invalid_url), None, None);
        assert!(png.is_none());

        // Negative cache hit within TTL
        assert!(resolver
            .try_resolve_cached(Some(invalid_url), None, None)
            .is_none());

        // Fast-forward negative cache failure timestamp by 10s to simulate TTL expiration without thread::sleep
        let key = derive_skin_key(Some(invalid_url), None, None);
        resolver.backdate_negative_cache_entry_for_test(&key, 10);

        // After TTL expired, negative cache allows retry
        assert!(resolver
            .try_resolve_cached(Some(invalid_url), None, None)
            .is_none());
    }

    #[test]
    fn test_index_mapping_and_zero_second_cache_hit() {
        let temp_dir =
            std::env::temp_dir().join(format!("mtk_test_skin_idx_{}", current_unix_timestamp()));
        let _ = fs::create_dir_all(&temp_dir);

        let fake_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xAA, 0xBB];
        let hash = compute_sha256_hex(&fake_png);
        let png_path = temp_dir.join(format!("{}.png", hash));
        fs::write(&png_path, &fake_png).unwrap();

        // 1. Resolver opens directory, should record hash into index
        let resolver = SkinResolver::new(Some(temp_dir.clone()));
        assert!(resolver.index.read().unwrap().skins.contains_key(&hash));

        // 2. Associate player and UUID with this hash
        {
            let mut idx = resolver.index.write().unwrap();
            idx.record_mapping(
                &hash,
                None,
                Some("eea5f812-2bde-3362-93f3-688fdbf6ab6e"),
                Some("JaxLocke"),
                1000,
            );
            idx.save_to_disk(&temp_dir).unwrap();
        }

        // 3. New resolver in another session: 0-second cache hit by name and offline UUID
        let resolver2 = SkinResolver::new(Some(temp_dir.clone()));
        let resolved_by_name = resolver2.try_resolve_cached(None, None, Some("jaxlocke"));
        assert_eq!(resolved_by_name, Some(fake_png.clone()));

        let resolved_by_uuid =
            resolver2.try_resolve_cached(None, Some("uuid_eea5f8122bde336293f3688fdbf6ab6e"), None);
        assert_eq!(resolved_by_uuid, Some(fake_png));

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_legacy_file_automatic_migration() {
        let temp_dir =
            std::env::temp_dir().join(format!("mtk_test_skin_mig_{}", current_unix_timestamp()));
        let _ = fs::create_dir_all(&temp_dir);

        let fake_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x11, 0x22];
        let hash = compute_sha256_hex(&fake_png);

        // Create legacy files
        let legacy_player = temp_dir.join("player_jaxlocke.png");
        let legacy_uuid = temp_dir.join("uuid_eea5f8122bde336293f3688fdbf6ab6e.png");
        fs::write(&legacy_player, &fake_png).unwrap();
        fs::write(&legacy_uuid, &fake_png).unwrap();

        // Instantiate resolver: load_and_migrate runs
        let resolver = SkinResolver::new(Some(temp_dir.clone()));

        // Legacy files should be removed and migrated to <hash>.png
        assert!(!legacy_player.exists());
        assert!(!legacy_uuid.exists());
        assert!(temp_dir.join(format!("{}.png", hash)).exists());

        // Check index contains both mappings
        let idx = resolver.index.read().unwrap();
        assert_eq!(idx.players.get("jaxlocke"), Some(&hash));
        assert_eq!(
            idx.uuids.get("eea5f8122bde336293f3688fdbf6ab6e"),
            Some(&hash)
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn test_resolve_real_mojang_skin_texture() {
        require_online!();
        let resolver = SkinResolver::new(None);
        let real_url = "http://textures.minecraft.net/texture/5813943ca53225ee0ee814321cba1e420ea21f7c55f2998f9306cfad6655f69e";
        let png = resolver.resolve_skin_png(Some(real_url), None, None);
        if let Some(bytes) = png {
            assert!(is_valid_png_header(&bytes));
            assert_eq!(bytes.len(), 3648);
        }
    }

    #[test]
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn test_resolve_skin_by_player_name_or_uuid_via_mirror() {
        require_online!();
        let resolver = SkinResolver::new(None);
        // Test Notch skin resolution via mirror/Mojang
        let png = resolver.resolve_skin_png(None, None, Some("Notch"));
        if let Some(bytes) = png {
            assert!(is_valid_png_header(&bytes));
        }
    }

    #[test]
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn test_offline_uuid_prioritizes_name_and_rejects_alex_fallback() {
        require_online!();
        let resolver = SkinResolver::new(None);
        // "0b9c337af39a34ec91a0929852e7d393" is an offline/synthetic UUID (version 3)
        // With name "scvb1", it should resolve the authentic skin hash instead of Alex.
        if let Some(bytes) = resolver.resolve_skin_png(
            None,
            Some("0b9c337af39a34ec91a0929852e7d393"),
            Some("scvb1"),
        ) {
            assert!(is_valid_png_header(&bytes));
            let hash = compute_sha256_hex(&bytes);
            assert_ne!(
                hash, CRAFTHEAD_ALEX_HASH,
                "Must not fall back to Alex default skin!"
            );
            assert_ne!(
                hash, CRAFTHEAD_STEVE_HASH,
                "Must not fall back to Steve default skin!"
            );
            assert_eq!(
                hash,
                "225b43a4c1c0fe547178b145a301d61ddcd1515bdee5191f29b56e879b124fd4"
            );
        }
    }

    #[test]
    #[cfg(all(feature = "net", not(target_arch = "wasm32")))]
    fn test_offline_uuid_without_name_rejects_mirror_fallback() {
        require_online!();
        let resolver = SkinResolver::new(None);
        // Without name, offline UUID has no online profile. Mirror returns 200 with Alex fallback,
        // which our strict header & hash validation must reject as None.
        let png = resolver.resolve_skin_png(None, Some("0b9c337af39a34ec91a0929852e7d393"), None);
        assert!(png.is_none(), "Mirror fallback skin must be rejected!");
    }

    #[test]
    fn test_purge_poisoned_fallback_cache() {
        let temp_dir =
            std::env::temp_dir().join(format!("mtk_test_skin_purge_{}", current_unix_timestamp()));
        let _ = fs::create_dir_all(&temp_dir);

        let alex_hash = CRAFTHEAD_ALEX_HASH;
        let fake_alex_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xAA, 0xBB];
        let alex_path = temp_dir.join(format!("{}.png", alex_hash));
        fs::write(&alex_path, &fake_alex_png).unwrap();

        // Write poisoned index.json
        let poisoned_json = format!(
            r#"{{
              "version": 1,
              "skins": {{
                "{}": {{
                  "hash": "{}",
                  "skin_url": "https://crafthead.net/skin/0b9c337af39a34ec91a0929852e7d393",
                  "player_name": "scvb1",
                  "uuid": "0b9c337af39a34ec91a0929852e7d393",
                  "last_checked": 1000
                }}
              }},
              "players": {{
                "scvb1": "{}"
              }},
              "uuids": {{
                "0b9c337af39a34ec91a0929852e7d393": "{}"
              }}
            }}"#,
            alex_hash, alex_hash, alex_hash, alex_hash
        );
        fs::write(temp_dir.join("index.json"), poisoned_json).unwrap();

        // Resolver initialization triggers load_and_migrate
        let resolver = SkinResolver::new(Some(temp_dir.clone()));

        // 1. Poisoned entry must be removed from in-memory index
        let idx = resolver.index.read().unwrap();
        assert!(!idx.skins.contains_key(alex_hash));
        assert_eq!(idx.players.get("scvb1"), None);
        assert_eq!(idx.uuids.get("0b9c337af39a34ec91a0929852e7d393"), None);

        // 2. Poisoned PNG file must be deleted from disk
        assert!(!alex_path.exists());

        // 3. try_resolve_cached must return None
        assert!(resolver
            .try_resolve_cached(
                None,
                Some("0b9c337af39a34ec91a0929852e7d393"),
                Some("scvb1"),
            )
            .is_none());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_skin_circuit_breaker_trip_and_fast_fail() {
        let circuit = SkinCircuitBreaker::new(2, 60);
        assert!(!circuit.is_tripped());
        assert_eq!(circuit.failure_count(), 0);

        // First failure: below threshold
        circuit.record_transport_failure();
        assert!(!circuit.is_tripped());
        assert_eq!(circuit.failure_count(), 1);

        // Second failure: reaches threshold of 2 -> trips!
        circuit.record_transport_failure();
        assert!(circuit.is_tripped());

        // SkinResolver with tripped circuit fast-fails without hitting network
        let resolver = SkinResolver::new(None);
        resolver.circuit.trip();
        assert!(resolver.is_circuit_tripped());

        // Fast-fail: resolves instantly to None
        let res = resolver.resolve_skin_png(
            Some("http://textures.minecraft.net/texture/non_existent_12345"),
            None,
            None,
        );
        assert!(res.is_none());

        // Reset clears circuit
        resolver.circuit.reset();
        assert!(!resolver.is_circuit_tripped());
    }
}
