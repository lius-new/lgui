#[cfg(feature = "persistent-cache")]
use std::sync::Arc;

use super::*;

const MIB: usize = 1024 * 1024;

#[test]
fn default_policy_is_valid_and_concrete() {
    let options = MemoryOptions::default();
    assert!(options.validate().is_ok());
    assert_eq!(options.budget.cache_bytes, 64 * MIB);
    assert_eq!(
        options.default_image_cache_policy,
        ImageCachePolicy::WhileVisible
    );
    assert!(!options.persistent_cache_enabled);
}

#[test]
fn validation_rejects_application_default_image_policy() {
    let options = MemoryOptions::new(
        MemoryBudget::new(64 * MIB),
        ImageCachePolicy::ApplicationDefault,
        false,
    );
    assert_eq!(
        options.validate(),
        Err("application default image policy must be concrete")
    );
}

#[test]
fn unbounded_policy_uses_maximum_budget() {
    let options = MemoryOptions::unbounded(ImageCachePolicy::NoStore, false);
    assert!(options.validate().is_ok());
    assert_eq!(options.budget.cache_bytes, usize::MAX);
    assert_eq!(options.budget.persistent_bytes, u64::MAX);
}

#[test]
fn instance_ids_are_unique() {
    let governor = MemoryGovernor::new(test_memory_options());
    let first = governor.next_instance_id();
    let second = governor.next_instance_id();
    assert_ne!(first, second);
}

#[test]
fn transient_reservations_release_on_drop() {
    let governor = MemoryGovernor::new(test_memory_options());
    let first = governor.try_reserve(256 * MIB).expect("first reservation fits");
    let _second = governor.try_reserve(256 * MIB).expect("second reservation fits");
    assert!(governor.try_reserve(1).is_none(), "512 MiB ceiling enforced");
    drop(first);
    assert!(governor.try_reserve(256 * MIB).is_some());
}

#[test]
fn task_reservations_enforce_the_parallel_limit() {
    let governor = MemoryGovernor::new(test_memory_options());
    let mut held = Vec::new();
    for _ in 0..4 {
        held.push(governor.try_reserve_task(1).expect("task slot available"));
    }
    assert!(governor.try_reserve_task(1).is_none());
    assert_eq!(held[0].bytes(), 1);
    drop(held);
    assert!(governor.try_reserve_task(1).is_some());
}

#[cfg(feature = "persistent-cache")]
mod file_store {
    use super::*;
    use std::{fs, time::SystemTime};

    fn temporary_directory(label: &str) -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("lgui-{label}-{}-{unique}", std::process::id()))
    }

    #[test]
    fn persistent_store_round_trips_and_rejects_corruption() {
        let root = temporary_directory("persistent-roundtrip");
        let store = FileCacheStore::new(&root);
        let key = PersistentCacheKey::new("avatars", "https://example/avatar", 1);
        store
            .put(PersistentEntry::new(
                key.clone(),
                b"portable compressed bytes".to_vec(),
            ))
            .expect("cache put should succeed");
        drop(store);
        let store = FileCacheStore::new(&root);
        assert_eq!(
            store
                .get(&key)
                .expect("cache get should succeed")
                .unwrap()
                .bytes,
            b"portable compressed bytes"
        );

        let data = fs::read_dir(root.join("avatars"))
            .unwrap()
            .map(Result::unwrap)
            .find(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("data"))
            .unwrap()
            .path();
        fs::write(data, b"corrupt").unwrap();
        assert!(store.get(&key).expect("corruption is a miss").is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn persistent_store_enforces_quota_and_sensitive_boundary() {
        let root = temporary_directory("persistent-quota");
        let store = FileCacheStore::new(&root);
        for index in 0..3 {
            store
                .put(PersistentEntry::new(
                    PersistentCacheKey::new("public", format!("entry-{index}"), 1),
                    vec![index as u8; 16],
                ))
                .unwrap();
        }
        let stats = store.trim_to(16).unwrap();
        assert_eq!(stats.entry_count, 1);
        assert_eq!(stats.bytes, 16);

        let mut sensitive =
            PersistentEntry::new(PersistentCacheKey::new("auth", "token", 1), vec![1, 2, 3]);
        sensitive.sensitive = true;
        assert!(matches!(
            store.put(sensitive),
            Err(CacheStoreError::SensitiveEntry)
        ));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn governor_exposes_persistent_cache_and_trims_on_reconfiguration() {
        let root = temporary_directory("persistent-governor");
        let store = FileCacheStore::new(&root);
        for index in 0..2 {
            store
                .put(PersistentEntry::new(
                    PersistentCacheKey::new("public", format!("entry-{index}"), 1),
                    vec![7; 16],
                ))
                .unwrap();
        }
        let governor =
            test_memory_governor_with_store(test_memory_options(), Arc::new(store.clone()));
        assert!(governor.persistent_cache().is_some());
        assert_eq!(governor.persistent_cache_stats().unwrap().bytes, 32);

        let mut options = test_memory_options();
        options.budget.persistent_bytes = 16;
        governor.set_options(options);
        assert_eq!(governor.persistent_cache_stats().unwrap().bytes, 16);
        let _ = fs::remove_dir_all(root);
    }
}
