use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};

use super::*;

#[test]
fn application_policy_validation_rejects_framework_defaults_and_invalid_limits() {
    let options = test_memory_options();
    assert!(options.validate().is_ok());

    let mut invalid = options;
    invalid.budget.cache_soft_bytes = 2;
    invalid.budget.cache_hard_bytes = 1;
    assert_eq!(
        invalid.validate(),
        Err("memory cache soft budget exceeds hard budget")
    );

    let mut unresolved = options;
    unresolved.default_image_cache_policy = ImageCachePolicy::ApplicationDefault;
    assert_eq!(
        unresolved.validate(),
        Err("application default image policy must be concrete")
    );

    let mut unnamed = options;
    unnamed.policy_name = "";
    assert_eq!(
        unnamed.validate(),
        Err("memory policy name must not be empty")
    );

    let mut no_workers = options;
    no_workers.budget.max_parallel_large_tasks = 0;
    assert_eq!(
        no_workers.validate(),
        Err("memory policy must allow at least one large task")
    );

    let mut encoded_too_large = options;
    encoded_too_large.budget.max_encoded_resource_bytes =
        encoded_too_large.budget.transient_hard_bytes + 1;
    assert_eq!(
        encoded_too_large.validate(),
        Err("encoded resource limit exceeds transient hard budget")
    );

    let mut decoded_too_large = options;
    decoded_too_large.budget.max_decoded_resource_bytes =
        decoded_too_large.budget.transient_hard_bytes + 1;
    assert_eq!(
        decoded_too_large.validate(),
        Err("decoded resource limit exceeds transient hard budget")
    );
}

#[test]
fn image_request_default_is_resolved_by_each_application_policy() {
    let request = crate::core::ImageRequest::new(crate::core::UiImageSource::url(
        "https://example.invalid/image.png",
    ));
    let mut first = test_memory_options();
    first.default_image_cache_policy = ImageCachePolicy::NoStore;
    let mut second = test_memory_options();
    second.default_image_cache_policy = ImageCachePolicy::Session;

    assert_eq!(
        request.cache_policy_value(first.default_image_cache_policy),
        ImageCachePolicy::NoStore
    );
    assert_eq!(
        request.cache_policy_value(second.default_image_cache_policy),
        ImageCachePolicy::Session
    );
}

#[test]
fn registrations_are_application_scoped_and_unregister_on_drop() {
    let governor = MemoryGovernor::new(test_memory_options());
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
    let governor = MemoryGovernor::new(test_memory_options());
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
    let mut options = test_memory_options();
    options.budget.transient_hard_bytes = 16;
    options.budget.max_encoded_resource_bytes = 16;
    options.budget.max_decoded_resource_bytes = 16;
    let governor = MemoryGovernor::new(options);
    let first = governor.try_reserve(12).expect("first reservation fits");
    assert!(governor.try_reserve(5).is_none());
    assert_eq!(governor.snapshot().transient_reserved_bytes, 12);
    drop(first);
    assert_eq!(governor.snapshot().transient_reserved_bytes, 0);
}

#[test]
fn task_reservations_enforce_parallel_and_byte_limits() {
    let mut options = test_memory_options();
    options.budget.transient_hard_bytes = 32;
    options.budget.max_encoded_resource_bytes = 32;
    options.budget.max_decoded_resource_bytes = 32;
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
fn unbounded_transient_policy_does_not_serialize_maximum_sized_reservations() {
    let governor = MemoryGovernor::new(MemoryOptions::unbounded(ImageCachePolicy::NoStore, false));
    let first = governor
        .try_reserve_task(usize::MAX)
        .expect("first unbounded reservation");
    let second = governor
        .try_reserve_task(usize::MAX)
        .expect("second unbounded reservation");

    assert_eq!(first.bytes(), usize::MAX);
    assert_eq!(second.bytes(), usize::MAX);
    assert_eq!(governor.snapshot().transient_reserved_bytes, 0);
    drop((first, second));
}

#[test]
fn domain_budgets_are_supplied_by_the_application_policy() {
    let mut options = test_memory_options();
    options.domains.gdi_bytes = 40;
    options.domains.d2d_bytes = 24;
    let governor = MemoryGovernor::new(options);
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
    let second_gdi_budget = Arc::new(AtomicUsize::new(0));
    let second_gdi = register(CacheDomain::Gdi, Arc::clone(&second_gdi_budget));
    let _d2d = register(CacheDomain::D2d, Arc::clone(&d2d_budget));

    assert_eq!(gdi_budget.load(Ordering::Acquire), 20);
    assert_eq!(second_gdi_budget.load(Ordering::Acquire), 20);
    assert_eq!(d2d_budget.load(Ordering::Acquire), 24);
    drop(second_gdi);
    assert_eq!(gdi_budget.load(Ordering::Acquire), 40);
    drop(gdi);
}

#[test]
fn zero_domain_budget_is_forwarded_without_a_framework_minimum() {
    let mut options = test_memory_options();
    options.domains.encoded_image_bytes = 0;
    let governor = MemoryGovernor::new(options);
    let observed = Arc::new(AtomicUsize::new(usize::MAX));
    let set_observed = Arc::clone(&observed);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::EncodedImage,
        governor.next_instance_id(),
        "disabled-image-cache",
        CacheAdapter::managed(
            CacheUsage::default,
            |_| TrimResult::default(),
            move |budget| set_observed.store(budget, Ordering::Release),
        ),
    ));

    assert_eq!(observed.load(Ordering::Acquire), 0);
}

#[test]
fn lifecycle_actions_are_selected_by_the_application_policy() {
    let mut options = test_memory_options();
    let text_budget = options.domains.text_bytes;
    options.events.window_hidden = MemoryAction::trim(CacheScope::Memory, usize::MAX);
    options.events.window_shown = MemoryAction::None;
    let governor = MemoryGovernor::new(options);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let trim_observed = Arc::clone(&observed);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::Text,
        governor.next_instance_id(),
        "event-policy",
        CacheAdapter::new(CacheUsage::default, move |request| {
            trim_observed.lock().unwrap().push(request);
            TrimResult::default()
        }),
    ));

    governor.notify(MemoryEvent::WindowShown);
    assert!(observed.lock().unwrap().is_empty());
    governor.notify(MemoryEvent::WindowHidden);
    assert_eq!(
        observed.lock().unwrap().as_slice(),
        &[TrimRequest {
            reason: TrimReason::WindowHidden,
            scope: CacheScope::Memory,
            target_bytes: text_budget,
        }]
    );
}

#[test]
fn frame_budget_enforcement_uses_hysteresis_and_is_rate_limited() {
    let mut options = test_memory_options();
    options.budget.cache_soft_bytes = 100;
    options.budget.cache_hard_bytes = 120;
    options.events.frame_committed = MemoryAction::EnforceBudget;
    let usage = Arc::new(AtomicUsize::new(110));
    let trim_calls = Arc::new(AtomicUsize::new(0));
    let usage_probe = Arc::clone(&usage);
    let observed = Arc::clone(&trim_calls);
    let governor = MemoryGovernor::new(options);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::Text,
        governor.next_instance_id(),
        "frame-budget",
        CacheAdapter::new(
            move || CacheUsage {
                cache_bytes: usage_probe.load(Ordering::Acquire),
                ..Default::default()
            },
            move |_| {
                observed.fetch_add(1, Ordering::AcqRel);
                TrimResult::default()
            },
        ),
    ));

    governor.notify(MemoryEvent::FrameCommitted);
    assert_eq!(trim_calls.load(Ordering::Acquire), 0);

    usage.store(121, Ordering::Release);
    governor.notify(MemoryEvent::FrameCommitted);
    assert_eq!(
        trim_calls.load(Ordering::Acquire),
        0,
        "the next frame inside the check interval must not rescan or trim"
    );
}

#[test]
fn frame_budget_enforcement_ignores_unavoidable_pinned_overflow() {
    let mut options = test_memory_options();
    options.budget.cache_soft_bytes = 100;
    options.budget.cache_hard_bytes = 120;
    options.events.frame_committed = MemoryAction::EnforceBudget;
    let trim_calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&trim_calls);
    let governor = MemoryGovernor::new(options);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::DecodedImage,
        governor.next_instance_id(),
        "visible-images",
        CacheAdapter::new(
            || CacheUsage {
                cache_bytes: 256,
                pinned_bytes: 256,
                ..Default::default()
            },
            move |_| {
                observed.fetch_add(1, Ordering::AcqRel);
                TrimResult::default()
            },
        ),
    ));

    governor.notify(MemoryEvent::FrameCommitted);
    assert_eq!(trim_calls.load(Ordering::Acquire), 0);
}

#[test]
fn frame_budget_enforcement_does_not_target_protected_host_scene() {
    let mut options = test_memory_options();
    options.budget.cache_soft_bytes = 100;
    options.budget.cache_hard_bytes = 120;
    options.events.frame_committed = MemoryAction::EnforceBudget;
    let trim_calls = Arc::new(AtomicUsize::new(0));
    let observed = Arc::clone(&trim_calls);
    let governor = MemoryGovernor::new(options);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::HostScene,
        governor.next_instance_id(),
        "visible-host-scene",
        CacheAdapter::new(
            || CacheUsage {
                rebuildable_bytes: 256,
                ..Default::default()
            },
            move |_| {
                observed.fetch_add(1, Ordering::AcqRel);
                TrimResult::default()
            },
        ),
    ));

    governor.notify(MemoryEvent::FrameCommitted);
    assert_eq!(trim_calls.load(Ordering::Acquire), 0);
}

#[test]
fn frame_budget_enforcement_trims_evictable_bytes_above_hard_limit() {
    let mut options = test_memory_options();
    options.budget.cache_soft_bytes = 100;
    options.budget.cache_hard_bytes = 120;
    options.domains.text_bytes = 100;
    options.events.frame_committed = MemoryAction::EnforceBudget;
    let trim_requests = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&trim_requests);
    let governor = MemoryGovernor::new(options);
    let _registration = governor.register(DomainRegistration::new(
        CacheDomain::Text,
        governor.next_instance_id(),
        "evictable-text",
        CacheAdapter::new(
            || CacheUsage {
                cache_bytes: 121,
                ..Default::default()
            },
            move |request| {
                observed.lock().unwrap().push(request);
                TrimResult::default()
            },
        ),
    ));

    governor.notify(MemoryEvent::FrameCommitted);
    governor.notify(MemoryEvent::FrameCommitted);
    assert_eq!(
        trim_requests.lock().unwrap().as_slice(),
        &[TrimRequest {
            reason: TrimReason::HardBudget,
            scope: CacheScope::Memory,
            target_bytes: 100,
        }]
    );
}

#[test]
fn finite_trim_targets_follow_active_application_domain_budgets() {
    let mut options = test_memory_options();
    options.budget.cache_soft_bytes = 100;
    options.budget.cache_hard_bytes = 120;
    options.domains.text_bytes = 60;
    options.domains.gdi_bytes = 40;
    let governor = MemoryGovernor::new(options);
    let observed = Arc::new(Mutex::new(Vec::new()));
    let register = |domain| {
        let observed = Arc::clone(&observed);
        governor.register(DomainRegistration::new(
            domain,
            governor.next_instance_id(),
            domain.as_str(),
            CacheAdapter::new(CacheUsage::default, move |request| {
                observed
                    .lock()
                    .unwrap()
                    .push((domain, request.target_bytes));
                TrimResult::default()
            }),
        ))
    };
    let _text = register(CacheDomain::Text);
    let _gdi = register(CacheDomain::Gdi);

    governor.trim(TrimReason::Explicit, CacheScope::Memory, 50);
    let mut targets = observed.lock().unwrap().clone();
    targets.sort_by_key(|(domain, _)| *domain);

    assert_eq!(
        targets,
        vec![(CacheDomain::Text, 30), (CacheDomain::Gdi, 20)]
    );
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
    let governor = MemoryGovernor::new(test_memory_options());
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

    #[test]
    fn persistent_trim_scope_does_not_touch_memory_domains() {
        let root = temporary_directory("persistent-scope");
        let store = FileCacheStore::new(&root);
        store
            .put(PersistentEntry::new(
                PersistentCacheKey::new("public", "entry", 1),
                vec![7; 16],
            ))
            .unwrap();
        let mut options = test_memory_options();
        options.persistent_cache_enabled = false;
        let governor = MemoryGovernor::with_store(options, Some(Arc::new(store.clone())));
        let memory_trims = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&memory_trims);
        let _registration = governor.register(DomainRegistration::new(
            CacheDomain::EncodedImage,
            governor.next_instance_id(),
            "memory-cache",
            CacheAdapter::new(CacheUsage::default, move |_| {
                observed.fetch_add(1, Ordering::SeqCst);
                TrimResult::default()
            }),
        ));

        governor.trim(TrimReason::Explicit, CacheScope::Memory, 0);
        assert_eq!(memory_trims.load(Ordering::SeqCst), 1);
        assert_eq!(store.stats().unwrap().entry_count, 1);

        assert_eq!(
            governor.trim(TrimReason::Explicit, CacheScope::Persistent, 0),
            16
        );
        assert_eq!(memory_trims.load(Ordering::SeqCst), 1);
        assert_eq!(store.stats().unwrap().entry_count, 0);
        let _ = fs::remove_dir_all(root);
    }
}
