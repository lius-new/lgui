use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};

use super::*;

#[test]
fn profiles_keep_soft_hard_and_transient_budgets_ordered() {
    for options in [
        MemoryOptions::low_memory(),
        MemoryOptions::balanced(),
        MemoryOptions::performance(),
    ] {
        assert!(options.budget.cpu_cache_hard_bytes > options.budget.cpu_cache_soft_bytes);
        assert!(options.budget.native_cache_hard_bytes > options.budget.native_cache_soft_bytes);
        assert!(options.budget.transient_hard_bytes > 0);
    }
}

#[test]
fn registrations_are_application_scoped_and_unregister_on_drop() {
    let governor = MemoryGovernor::default();
    let registration = governor.register(DomainRegistration::new(
        CacheDomain::EncodedImage,
        governor.next_instance_id(),
        "application",
        CacheAdapter::new(
            || CacheUsage {
                cache_bytes: 1024,
                cpu_bytes: 1024,
                entries: 1,
                ..Default::default()
            },
            |_| TrimResult::default(),
        ),
    ));

    let snapshot = governor.snapshot();
    assert_eq!(snapshot.domains.len(), 1);
    assert_eq!(snapshot.usage.cache_bytes, 1024);
    drop(registration);
    assert!(governor.snapshot().domains.is_empty());
}

#[test]
fn trim_callbacks_run_without_holding_the_registry_lock() {
    let governor = MemoryGovernor::default();
    let nested = governor.clone();
    let calls = Arc::new(AtomicUsize::new(0));
    let trim_calls = Arc::clone(&calls);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::Skia,
        governor.next_instance_id(),
        "window:main",
        CacheAdapter::new(
            || CacheUsage::default(),
            move |_| {
                let _ = nested.snapshot();
                trim_calls.fetch_add(1, Ordering::SeqCst);
                TrimResult {
                    before_bytes: 10,
                    after_bytes: 3,
                }
            },
        ),
    ));

    assert_eq!(
        governor.trim(TrimReason::Explicit, CacheScope::Memory, 0),
        7
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        governor
            .snapshot()
            .last_trim
            .map(|trim| trim.released_bytes),
        Some(7)
    );
}

#[test]
fn reservations_enforce_the_transient_hard_limit_and_release_on_drop() {
    let mut options = MemoryOptions::low_memory();
    options.budget.transient_hard_bytes = 16;
    let governor = MemoryGovernor::new(options);
    let first = governor.try_reserve(12).expect("first reservation fits");
    assert!(governor.try_reserve(5).is_none());
    assert_eq!(governor.snapshot().transient_reserved_bytes, 12);
    drop(first);
    assert_eq!(governor.snapshot().transient_reserved_bytes, 0);
}

#[test]
fn task_reservations_enforce_parallel_and_byte_limits() {
    let mut options = MemoryOptions::low_memory();
    options.budget.transient_hard_bytes = 32;
    options.budget.max_parallel_large_tasks = 1;
    let governor = MemoryGovernor::new(options);
    let first = governor
        .try_reserve_task(16)
        .expect("first task reservation fits");
    assert!(governor.try_reserve_task(1).is_none());
    let snapshot = governor.snapshot();
    assert_eq!(snapshot.transient_reserved_bytes, 16);
    assert_eq!(snapshot.large_tasks_in_flight, 1);
    drop(first);
    assert!(governor.try_reserve_task(32).is_some());
}

#[test]
fn native_domain_budgets_share_the_application_total() {
    let governor = MemoryGovernor::new(MemoryOptions::low_memory());
    let gdi_budget = Arc::new(AtomicUsize::new(0));
    let d2d_budget = Arc::new(AtomicUsize::new(0));
    let register = |domain, budget: Arc<AtomicUsize>| {
        governor.register(DomainRegistration::new(
            domain,
            governor.next_instance_id(),
            domain.as_str(),
            CacheAdapter::managed(
                CacheUsage::default,
                |_| TrimResult::default(),
                move |value| {
                    budget.store(value, Ordering::Release);
                },
            ),
        ))
    };
    let gdi = register(CacheDomain::Gdi, Arc::clone(&gdi_budget));
    let _d2d = register(CacheDomain::D2d, Arc::clone(&d2d_budget));

    let total = gdi_budget
        .load(Ordering::Acquire)
        .saturating_add(d2d_budget.load(Ordering::Acquire));
    assert!(total <= governor.options().budget.native_cache_soft_bytes);
    drop(gdi);
}

#[test]
fn native_trim_drops_resources_on_the_adapter_owner_thread() {
    enum OwnerCommand {
        Trim,
        Stop,
    }

    struct OwnerResource {
        dropped: mpsc::Sender<std::thread::ThreadId>,
    }

    impl Drop for OwnerResource {
        fn drop(&mut self) {
            let _ = self.dropped.send(std::thread::current().id());
        }
    }

    let usage = Arc::new(Mutex::new(CacheUsage {
        cache_bytes: 64,
        cpu_bytes: 64,
        entries: 1,
        ..Default::default()
    }));
    let (command_tx, command_rx) = mpsc::channel();
    let (owner_tx, owner_rx) = mpsc::channel();
    let (dropped_tx, dropped_rx) = mpsc::channel();
    let owner_usage = Arc::clone(&usage);
    let owner = std::thread::spawn(move || {
        let owner_id = std::thread::current().id();
        owner_tx.send(owner_id).unwrap();
        let mut resources = vec![OwnerResource {
            dropped: dropped_tx,
        }];
        while let Ok(command) = command_rx.recv() {
            match command {
                OwnerCommand::Trim => {
                    *owner_usage.lock().unwrap() = CacheUsage::default();
                    resources.clear();
                }
                OwnerCommand::Stop => break,
            }
        }
    });
    let owner_id = owner_rx.recv().unwrap();
    let governor = MemoryGovernor::default();
    let snapshot_usage = Arc::clone(&usage);
    let trim_usage = Arc::clone(&usage);
    let trim_tx = command_tx.clone();
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::Gdi,
        governor.next_instance_id(),
        "owner-thread-native",
        CacheAdapter::new(
            move || *snapshot_usage.lock().unwrap(),
            move |_| {
                let before = trim_usage.lock().unwrap().resident_bytes();
                trim_tx.send(OwnerCommand::Trim).unwrap();
                TrimResult {
                    before_bytes: before,
                    after_bytes: before,
                }
            },
        ),
    ));

    assert_eq!(
        governor.trim(TrimReason::Explicit, CacheScope::Memory, 0),
        0,
        "asynchronous owner-thread release is reported after dispatch"
    );
    assert_eq!(
        dropped_rx
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap(),
        owner_id
    );
    assert_eq!(usage.lock().unwrap().resident_bytes(), 0);
    command_tx.send(OwnerCommand::Stop).unwrap();
    owner.join().unwrap();
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
}
