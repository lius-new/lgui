use super::*;
use std::{
    cell::Cell,
    time::{Duration, Instant},
};

static IMAGE_CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5, 0x1C, 0x0C,
    0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x64, 0xF8, 0x0F, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xE3, 0x66, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

struct TestRemoteLoader;

impl crate::assets::RemoteImageLoader for TestRemoteLoader {
    fn load(&self, _url: &str) -> Result<crate::assets::AssetBytes, crate::assets::AssetError> {
        Ok(Arc::from(ONE_PIXEL_PNG))
    }
}

struct BlockingRemoteLoader {
    first_started: Arc<AtomicBool>,
    release_first: Arc<AtomicBool>,
}

impl crate::assets::RemoteImageLoader for BlockingRemoteLoader {
    fn load(&self, _url: &str) -> Result<crate::assets::AssetBytes, crate::assets::AssetError> {
        if !self.first_started.swap(true, Ordering::AcqRel) {
            while !self.release_first.load(Ordering::Acquire) {
                std::thread::yield_now();
            }
        }
        Ok(Arc::from(ONE_PIXEL_PNG))
    }
}

#[test]
fn decoded_image_hit_does_not_recreate_the_image() {
    let _serial = IMAGE_CACHE_TEST_LOCK
        .lock()
        .expect("image cache test lock poisoned");
    let _gdiplus = crate::platform::win32::gdiplus::GdiPlusRuntime::start()
        .expect("start GDI+ for image cache test");
    trim_decoded_image_cache(0);
    set_decoded_image_cache_budget(4);
    let creations = Cell::new(0);

    for _ in 0..2 {
        assert!(with_cached_decoded(
            "decoded-hit-does-not-read-encoded".to_owned(),
            || {
                creations.set(creations.get() + 1);
                decode_image(ONE_PIXEL_PNG)
            },
            |_| true,
        )
        .unwrap_or(false));
    }

    assert_eq!(creations.get(), 1);
    trim_decoded_image_cache(0);
    set_decoded_image_cache_budget(0);
}

#[test]
fn zero_budget_win32_cache_still_delivers_a_reachable_image() {
    let _serial = IMAGE_CACHE_TEST_LOCK
        .lock()
        .expect("image cache test lock poisoned");
    let _gdiplus = crate::platform::win32::gdiplus::GdiPlusRuntime::start()
        .expect("start GDI+ for image cache test");
    let mut options = crate::memory::test_memory_options();
    options.budget.max_parallel_large_tasks = 1;
    let governor = crate::memory::MemoryGovernor::new(options);
    let _memory = install_image_memory_governor(governor.clone());
    trim_cached_image_cache(0);
    let cache = portable_image_cache_handle();
    cache.set_budget(0);
    let _loader = install_remote_image_loader(crate::assets::RemoteImageLoaderHandle::new(
        TestRemoteLoader,
    ));
    let source = ImageSource::url("https://example.invalid/avatar.png");

    assert_eq!(request_cached_image(&source), CachedImageStatus::Loading);
    let deadline = Instant::now() + Duration::from_secs(2);
    while request_cached_image(&source) == CachedImageStatus::Loading && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(request_cached_image(&source), CachedImageStatus::Ready);
    while governor.snapshot().large_tasks_in_flight != 0 && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let _occupied_task = governor
        .try_reserve_task(1)
        .expect("image loading task slot should be available after load");
    let (first_bytes, width, height) = cached_image_data(&source).expect("cached remote image");
    let (second_bytes, _, _) = cached_image_data(&source).expect("cached remote image hit");
    assert_eq!((width, height), (1, 1));
    assert!(Arc::ptr_eq(&first_bytes, &second_bytes));
    assert_eq!(cache.stats().pinned_bytes, ONE_PIXEL_PNG.len());
    trim_cached_image_cache(0);
}

#[test]
fn images_waiting_for_a_task_slot_are_loaded_in_order() {
    let _serial = IMAGE_CACHE_TEST_LOCK
        .lock()
        .expect("image cache test lock poisoned");
    let _gdiplus = crate::platform::win32::gdiplus::GdiPlusRuntime::start()
        .expect("start GDI+ for image cache test");
    let mut options = crate::memory::test_memory_options();
    options.budget.max_parallel_large_tasks = 1;
    let _memory = install_image_memory_governor(crate::memory::MemoryGovernor::new(options));
    trim_cached_image_cache(0);
    let cache = portable_image_cache_handle();
    cache.set_budget(options.domains.encoded_image_bytes);
    let first_started = Arc::new(AtomicBool::new(false));
    let release_first = Arc::new(AtomicBool::new(false));
    let _loader = install_remote_image_loader(crate::assets::RemoteImageLoaderHandle::new(
        BlockingRemoteLoader {
            first_started: Arc::clone(&first_started),
            release_first: Arc::clone(&release_first),
        },
    ));
    let sources = [
        ImageSource::url("https://example.invalid/avatar-1.png"),
        ImageSource::url("https://example.invalid/avatar-2.png"),
        ImageSource::url("https://example.invalid/avatar-3.png"),
    ];

    for source in &sources {
        assert_eq!(request_cached_image(source), CachedImageStatus::Loading);
    }
    assert_eq!(
        request_cached_image(&sources[1]),
        CachedImageStatus::Loading,
        "a saturated task limit must queue the image instead of failing it"
    );
    release_first.store(true, Ordering::Release);

    let deadline = Instant::now() + Duration::from_secs(2);
    while sources
        .iter()
        .any(|source| request_cached_image(source) != CachedImageStatus::Ready)
        && Instant::now() < deadline
    {
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(first_started.load(Ordering::Acquire));
    for source in &sources {
        assert_eq!(request_cached_image(source), CachedImageStatus::Ready);
    }
    trim_cached_image_cache(0);
}
