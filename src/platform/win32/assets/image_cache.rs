use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    fs,
    io::Cursor,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, LazyLock, Mutex, OnceLock,
    },
};

use windows::Win32::{
    Foundation::{HWND, LPARAM, RECT, WPARAM},
    Graphics::{
        Gdi::HDC,
        GdiPlus::{
            GdipCreateFromHDC, GdipDeleteGraphics, GdipDisposeImage, GdipDrawImageRectRectI,
            GdipGetImageHeight, GdipGetImageWidth, GdipLoadImageFromStream, GpGraphics, GpImage,
            Ok as GpOk, UnitPixel,
        },
    },
    UI::{
        Shell::SHCreateMemStream,
        WindowsAndMessaging::{PostMessageW, WM_APP},
    },
};

pub const WM_IMAGE_CACHE_INVALIDATED: u32 = WM_APP + 42;
const MAX_IMAGE_CACHE_ENTRIES: usize = 4096;

static IMAGE_CACHE: LazyLock<Mutex<ImageCache>> =
    LazyLock::new(|| Mutex::new(ImageCache::default()));
static IMAGE_CACHE_EPOCH: AtomicU64 = AtomicU64::new(0);
static IMAGE_REPAINT_HWND: LazyLock<Mutex<Option<isize>>> = LazyLock::new(|| Mutex::new(None));
static IMAGE_REPAINT_PENDING: AtomicBool = AtomicBool::new(false);
static IMAGE_INVALIDATED_REQUEST_KEYS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static REMOTE_IMAGE_LOADER: LazyLock<Mutex<Option<crate::assets::RemoteImageLoaderHandle>>> =
    LazyLock::new(|| Mutex::new(None));
static IMAGE_MEMORY_GOVERNOR: LazyLock<Mutex<Option<crate::memory::MemoryGovernor>>> =
    LazyLock::new(|| Mutex::new(None));
static IMAGE_REQUESTS: LazyLock<Mutex<HashMap<String, crate::core::ImageRequest>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static IMAGE_LOAD_QUEUE: LazyLock<Mutex<VecDeque<PendingImageLoad>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
static IMAGE_REACHABILITY: LazyLock<
    Mutex<HashMap<crate::memory::DomainInstanceId, HashSet<String>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn decoded_image_telemetry() -> &'static crate::memory::CacheTelemetry {
    static TELEMETRY: OnceLock<crate::memory::CacheTelemetry> = OnceLock::new();
    TELEMETRY.get_or_init(Default::default)
}

thread_local! {
    static DECODED_IMAGE_CACHE: RefCell<crate::memory::LruCache<String, DecodedImage>> = RefCell::new(
        crate::memory::LruCache::new(
            0,
            crate::memory::ResourceClass::Cache,
            decoded_image_telemetry().clone(),
        )
    );
}

pub(crate) struct RemoteImageLoaderGuard {
    previous: Option<crate::assets::RemoteImageLoaderHandle>,
}

pub(crate) struct ImageMemoryGovernorGuard {
    previous: Option<crate::memory::MemoryGovernor>,
}

impl Drop for ImageMemoryGovernorGuard {
    fn drop(&mut self) {
        *IMAGE_MEMORY_GOVERNOR
            .lock()
            .expect("image memory governor poisoned") = self.previous.take();
    }
}

pub(crate) fn install_image_memory_governor(
    governor: crate::memory::MemoryGovernor,
) -> ImageMemoryGovernorGuard {
    let previous = IMAGE_MEMORY_GOVERNOR
        .lock()
        .expect("image memory governor poisoned")
        .replace(governor);
    ImageMemoryGovernorGuard { previous }
}

fn application_image_cache_policy() -> crate::core::ImageCachePolicy {
    IMAGE_MEMORY_GOVERNOR
        .lock()
        .expect("image memory governor poisoned")
        .as_ref()
        .map(|governor| governor.options().default_image_cache_policy)
        .unwrap_or(crate::core::ImageCachePolicy::NoStore)
}

impl Drop for RemoteImageLoaderGuard {
    fn drop(&mut self) {
        *REMOTE_IMAGE_LOADER
            .lock()
            .expect("remote image loader lock poisoned") = self.previous.take();
    }
}

pub(crate) fn install_remote_image_loader(
    loader: crate::assets::RemoteImageLoaderHandle,
) -> RemoteImageLoaderGuard {
    let previous = REMOTE_IMAGE_LOADER
        .lock()
        .expect("remote image loader lock poisoned")
        .replace(loader);
    RemoteImageLoaderGuard { previous }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
pub enum ImageFit {
    Cover,
    Contain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CachedImageStatus {
    Loading,
    Ready,
    Failed,
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum ImageSource {
    Url(String),
    File(PathBuf),
    Asset { key: String, bytes: &'static [u8] },
}

impl ImageSource {
    pub fn url(value: impl Into<String>) -> Self {
        Self::Url(value.into())
    }

    #[allow(dead_code)]
    pub fn file(value: impl Into<PathBuf>) -> Self {
        Self::File(value.into())
    }

    #[allow(dead_code)]
    pub fn asset(key: impl Into<String>, bytes: &'static [u8]) -> Self {
        Self::Asset {
            key: key.into(),
            bytes,
        }
    }

    fn key(&self) -> String {
        match self {
            Self::Url(value) => format!("url:{value}"),
            Self::File(path) => format!("file:{}", path.display()),
            Self::Asset { key, .. } => format!("asset:{key}"),
        }
    }
}

struct ImageCache {
    entries: HashMap<String, CachedImage>,
    resident_bytes: usize,
    budget_bytes: usize,
    tick: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

struct CachedImage {
    status: CachedImageStatus,
    bytes: Option<crate::assets::AssetBytes>,
    width: i32,
    height: i32,
    resident_bytes: usize,
    last_used: u64,
    request_key: String,
    policy: crate::core::ImageCachePolicy,
    priority: crate::memory::CachePriority,
    reachable: bool,
}

struct PendingImageLoad {
    key: String,
    source: ImageSource,
    request_key: String,
    epoch: u64,
}

impl Default for ImageCache {
    fn default() -> Self {
        Self {
            entries: HashMap::new(),
            resident_bytes: 0,
            budget_bytes: 0,
            tick: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
}

impl ImageCache {
    fn next_tick(&mut self) -> u64 {
        self.tick = self.tick.wrapping_add(1);
        self.tick
    }

    fn insert(&mut self, key: String, mut entry: CachedImage) {
        if let Some(previous) = self.entries.remove(&key) {
            self.resident_bytes = self.resident_bytes.saturating_sub(previous.resident_bytes);
        }
        entry.last_used = self.next_tick();
        self.resident_bytes = self.resident_bytes.saturating_add(entry.resident_bytes);
        self.entries.insert(key, entry);
        self.evict_to(self.budget_bytes, MAX_IMAGE_CACHE_ENTRIES, true);
    }

    fn evict_to(&mut self, target_bytes: usize, max_entries: usize, preserve_reachable: bool) {
        while self.resident_bytes > target_bytes || self.entries.len() > max_entries {
            let Some(key) = self
                .entries
                .iter()
                .filter(|(_, entry)| {
                    entry.status != CachedImageStatus::Loading
                        && (!preserve_reachable || !entry.reachable)
                })
                .min_by_key(|(_, entry)| (entry.reachable, entry.priority, entry.last_used))
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(entry) = self.entries.remove(&key) {
                self.resident_bytes = self.resident_bytes.saturating_sub(entry.resident_bytes);
                self.evictions = self.evictions.saturating_add(1);
            }
        }
    }
}

struct DecodedImage {
    image: *mut GpImage,
    width: i32,
    height: i32,
}

impl Drop for DecodedImage {
    fn drop(&mut self) {
        if !self.image.is_null() {
            unsafe {
                let _ = GdipDisposeImage(self.image);
            }
        }
    }
}

pub fn register_image_repaint_hwnd(hwnd: HWND) {
    {
        let mut target = IMAGE_REPAINT_HWND
            .lock()
            .expect("image repaint hwnd poisoned");
        *target = Some(hwnd.0 as isize);
    }
    schedule_image_cache_repaint();
}

pub fn clear_image_repaint_hwnd(hwnd: HWND) {
    let mut target = IMAGE_REPAINT_HWND
        .lock()
        .expect("image repaint hwnd poisoned");
    if *target == Some(hwnd.0 as isize) {
        *target = None;
        IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
    }
}

pub(crate) fn take_image_cache_invalidations() -> HashSet<String> {
    IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
    std::mem::take(
        &mut *IMAGE_INVALIDATED_REQUEST_KEYS
            .lock()
            .expect("image invalidation queue poisoned"),
    )
}

pub(crate) fn decoded_image_cache_usage() -> crate::memory::CacheUsage {
    decoded_image_telemetry().snapshot()
}

pub(crate) fn trim_decoded_image_cache(target_bytes: usize) -> usize {
    DECODED_IMAGE_CACHE.with(|cache| cache.borrow_mut().trim_to(target_bytes))
}

pub(crate) fn set_decoded_image_cache_budget(budget_bytes: usize) {
    DECODED_IMAGE_CACHE.with(|cache| cache.borrow_mut().set_budget(budget_bytes));
}

pub fn request_cached_image(source: &ImageSource) -> CachedImageStatus {
    let key = source.key();
    let request_key = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .map_or_else(|| key.clone(), crate::core::ImageRequest::cache_key);
    let application_policy = application_image_cache_policy();
    let (policy, priority) = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .map_or(
            (application_policy, crate::memory::CachePriority::Normal),
            |request| {
                (
                    request.cache_policy_value(application_policy),
                    request.priority_value(),
                )
            },
        );
    let epoch = IMAGE_CACHE_EPOCH.load(Ordering::Acquire);
    let mut should_start = false;
    let status = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        let tick = cache.next_tick();
        if cache
            .entries
            .get(&key)
            .is_some_and(|entry| entry.request_key != request_key)
        {
            if let Some(entry) = cache.entries.remove(&key) {
                cache.resident_bytes = cache.resident_bytes.saturating_sub(entry.resident_bytes);
                cache.evictions = cache.evictions.saturating_add(1);
            }
        }
        match cache.entries.get_mut(&key) {
            Some(entry) => {
                entry.last_used = tick;
                entry.policy = policy;
                entry.priority = priority;
                entry.reachable = true;
                let status = entry.status;
                cache.hits = cache.hits.saturating_add(1);
                status
            }
            None => {
                cache.misses = cache.misses.saturating_add(1);
                cache.insert(
                    key.clone(),
                    CachedImage {
                        status: CachedImageStatus::Loading,
                        bytes: None,
                        width: 0,
                        height: 0,
                        resident_bytes: 0,
                        last_used: tick,
                        request_key: request_key.clone(),
                        policy,
                        priority,
                        reachable: true,
                    },
                );
                should_start = true;
                CachedImageStatus::Loading
            }
        }
    };

    if should_start {
        start_image_load(key, source.clone(), request_key, epoch);
    }

    status
}

pub(crate) fn portable_image_cache_handle() -> crate::assets::ImageCacheHandle {
    crate::assets::ImageCacheHandle::new_managed(
        |request| {
            let source = match request.source() {
                crate::core::UiImageSource::Url(url) => ImageSource::url(url),
                crate::core::UiImageSource::File(path) => ImageSource::file(path),
                crate::core::UiImageSource::Static(_)
                | crate::core::UiImageSource::Bytes { .. } => {
                    return crate::assets::ImageStatus::Ready
                }
            };
            IMAGE_REQUESTS
                .lock()
                .expect("image request registry poisoned")
                .insert(source.key(), request.clone());
            portable_status(request_cached_image(&source))
        },
        |request| {
            let source = match request.source() {
                crate::core::UiImageSource::Url(url) => ImageSource::url(url),
                crate::core::UiImageSource::File(path) => ImageSource::file(path),
                crate::core::UiImageSource::Static(_)
                | crate::core::UiImageSource::Bytes { .. } => return None,
            };
            cached_image_data(&source).map(|(bytes, _, _)| bytes)
        },
        || {
            let cache = IMAGE_CACHE.lock().expect("image cache poisoned");
            crate::assets::ImageCacheStats {
                entries: cache.entries.len(),
                resident_bytes: cache.resident_bytes,
                pinned_bytes: cache
                    .entries
                    .values()
                    .filter(|entry| entry.reachable)
                    .map(|entry| entry.resident_bytes)
                    .sum(),
                budget_bytes: cache.budget_bytes,
                hits: cache.hits,
                misses: cache.misses,
                evictions: cache.evictions,
                largest_entry_bytes: cache
                    .entries
                    .values()
                    .map(|entry| entry.resident_bytes)
                    .max()
                    .unwrap_or(0),
                in_flight: cache
                    .entries
                    .values()
                    .filter(|entry| entry.status == CachedImageStatus::Loading)
                    .count(),
            }
        },
        trim_cached_image_cache,
        |budget| {
            let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
            cache.budget_bytes = budget;
            let target = cache.budget_bytes;
            cache.evict_to(target, MAX_IMAGE_CACHE_ENTRIES, true);
        },
        |owner, requests| {
            let mut active = HashSet::new();
            let mut registry = IMAGE_REQUESTS
                .lock()
                .expect("image request registry poisoned");
            for request in requests {
                let source_key = match request.source() {
                    crate::core::UiImageSource::Url(url) => ImageSource::url(url).key(),
                    crate::core::UiImageSource::File(path) => ImageSource::file(path).key(),
                    crate::core::UiImageSource::Static(_)
                    | crate::core::UiImageSource::Bytes { .. } => continue,
                };
                active.insert(source_key.clone());
                registry.insert(source_key, request.clone());
            }
            drop(registry);

            let reachable = {
                let mut owners = IMAGE_REACHABILITY
                    .lock()
                    .expect("image reachability poisoned");
                if active.is_empty() {
                    owners.remove(&owner);
                } else {
                    owners.insert(owner, active);
                }
                owners.values().flatten().cloned().collect::<HashSet<_>>()
            };

            let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
            let evict = cache
                .entries
                .iter_mut()
                .filter_map(|(key, entry)| {
                    entry.reachable = reachable.contains(key);
                    (!entry.reachable
                        && matches!(
                            entry.policy,
                            crate::core::ImageCachePolicy::NoStore
                                | crate::core::ImageCachePolicy::WhileVisible
                        ))
                    .then(|| key.clone())
                })
                .collect::<Vec<_>>();
            for key in evict {
                if let Some(entry) = cache.entries.remove(&key) {
                    cache.resident_bytes =
                        cache.resident_bytes.saturating_sub(entry.resident_bytes);
                    cache.evictions = cache.evictions.saturating_add(1);
                }
            }
            let target = cache.budget_bytes;
            cache.evict_to(target, MAX_IMAGE_CACHE_ENTRIES, true);
            let retained = cache.entries.keys().cloned().collect::<HashSet<_>>();
            drop(cache);
            IMAGE_REQUESTS
                .lock()
                .expect("image request registry poisoned")
                .retain(|key, _| retained.contains(key) || reachable.contains(key));
        },
    )
}

fn trim_cached_image_cache(target_bytes: usize) -> usize {
    if target_bytes == 0 {
        IMAGE_CACHE_EPOCH.fetch_add(1, Ordering::AcqRel);
    }
    let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
    let before = cache.resident_bytes;
    cache.evict_to(target_bytes, usize::MAX, target_bytes != 0);
    let released = before.saturating_sub(cache.resident_bytes);
    if target_bytes == 0 {
        cache.entries.clear();
        cache.resident_bytes = 0;
        drop(cache);
        IMAGE_REACHABILITY
            .lock()
            .expect("image reachability poisoned")
            .clear();
        IMAGE_REQUESTS
            .lock()
            .expect("image request registry poisoned")
            .clear();
        IMAGE_LOAD_QUEUE
            .lock()
            .expect("image load queue poisoned")
            .clear();
        IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
    }
    released
}

fn portable_status(status: CachedImageStatus) -> crate::assets::ImageStatus {
    match status {
        CachedImageStatus::Loading => crate::assets::ImageStatus::Loading,
        CachedImageStatus::Ready => crate::assets::ImageStatus::Ready,
        CachedImageStatus::Failed => crate::assets::ImageStatus::Failed,
    }
}

pub fn cached_image_data(source: &ImageSource) -> Option<(crate::assets::AssetBytes, i32, i32)> {
    let key = source.key();
    let entry = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        let tick = cache.next_tick();
        let Some(entry) = cache.entries.get_mut(&key) else {
            drop(cache);
            let _ = request_cached_image(source);
            return None;
        };
        if entry.status != CachedImageStatus::Ready {
            return None;
        }
        let Some(bytes) = entry.bytes.as_ref() else {
            return None;
        };
        entry.last_used = tick;
        let result = (bytes.clone(), entry.width, entry.height);
        if entry.policy == crate::core::ImageCachePolicy::NoStore {
            if let Some(entry) = cache.entries.remove(&key) {
                cache.resident_bytes = cache.resident_bytes.saturating_sub(entry.resident_bytes);
                cache.evictions = cache.evictions.saturating_add(1);
            }
        }
        result
    };
    Some(entry)
}

pub fn draw_cached_image(hdc: HDC, rect: RECT, source: &ImageSource, fit: ImageFit) -> bool {
    let key = source.key();
    let entry = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        let tick = cache.next_tick();
        let Some(entry) = cache.entries.get_mut(&key) else {
            drop(cache);
            let _ = request_cached_image(source);
            return false;
        };
        if entry.status != CachedImageStatus::Ready {
            return false;
        }
        let Some(bytes) = entry.bytes.as_ref() else {
            return false;
        };
        entry.last_used = tick;
        let result = (bytes.clone(), entry.width, entry.height);
        if entry.policy == crate::core::ImageCachePolicy::NoStore {
            if let Some(entry) = cache.entries.remove(&key) {
                cache.resident_bytes = cache.resident_bytes.saturating_sub(entry.resident_bytes);
                cache.evictions = cache.evictions.saturating_add(1);
            }
        }
        result
    };

    draw_image_bytes(hdc, rect, &key, &entry.0, entry.1, entry.2, fit)
}

fn start_image_load(key: String, source: ImageSource, request_key: String, epoch: u64) {
    if let ImageSource::Asset { bytes, .. } = &source {
        finish_image_load(key, request_key, Arc::from(*bytes), epoch);
        return;
    }
    let request = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .cloned();
    let governor = IMAGE_MEMORY_GOVERNOR
        .lock()
        .expect("image memory governor poisoned")
        .clone();
    let Some(governor) = governor else {
        fail_image_load(key, request_key, epoch);
        return;
    };
    let limit = governor.options().budget.max_encoded_resource_bytes;
    let Some(task_reservation) = governor.try_reserve_task(limit) else {
        IMAGE_LOAD_QUEUE
            .lock()
            .expect("image load queue poisoned")
            .push_back(PendingImageLoad {
                key,
                source,
                request_key,
                epoch,
            });
        return;
    };
    let loader = REMOTE_IMAGE_LOADER
        .lock()
        .expect("remote image loader lock poisoned")
        .clone();
    let failure_key = key.clone();
    let failure_request_key = request_key.clone();
    if std::thread::Builder::new()
        .name("lgui-image-loader".to_owned())
        .spawn(move || {
            let _task_reservation = task_reservation;
            let result = match source {
                ImageSource::File(path) => fs::read(path)
                    .map(Arc::<[u8]>::from)
                    .map_err(|error| crate::assets::AssetError::NotFound(error.to_string()))
                    .and_then(|bytes| crate::assets::validate_encoded_bytes(bytes, limit)),
                ImageSource::Url(url) => match (loader, request) {
                    (Some(loader), Some(request)) => {
                        crate::assets::load_url_image(&loader, &governor, &request, &url)
                    }
                    (Some(loader), None) => loader
                        .load(&url)
                        .and_then(|bytes| crate::assets::validate_encoded_bytes(bytes, limit)),
                    (None, _) => Err(crate::assets::AssetError::Unsupported(
                        "remote image loader is unavailable".to_owned(),
                    )),
                },
                ImageSource::Asset { .. } => unreachable!(),
            };
            match result {
                Ok(bytes) => finish_image_load(key, request_key, bytes, epoch),
                Err(_) => fail_image_load(key, request_key, epoch),
            }
            drop(_task_reservation);
            start_next_queued_image_load();
        })
        .is_err()
    {
        fail_image_load(failure_key, failure_request_key, epoch);
        start_next_queued_image_load();
    }
}

fn start_next_queued_image_load() {
    let next = loop {
        let Some(pending) = IMAGE_LOAD_QUEUE
            .lock()
            .expect("image load queue poisoned")
            .pop_front()
        else {
            return;
        };
        let current = pending.epoch == IMAGE_CACHE_EPOCH.load(Ordering::Acquire)
            && IMAGE_CACHE
                .lock()
                .expect("image cache poisoned")
                .entries
                .get(&pending.key)
                .is_some_and(|entry| {
                    entry.status == CachedImageStatus::Loading
                        && entry.request_key == pending.request_key
                });
        if current {
            break pending;
        }
    };
    start_image_load(next.key, next.source, next.request_key, next.epoch);
}

fn finish_image_load(
    key: String,
    request_key: String,
    bytes: crate::assets::AssetBytes,
    epoch: u64,
) {
    if !is_current_epoch(epoch) {
        return;
    }
    let request = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .cloned();
    let governor = IMAGE_MEMORY_GOVERNOR
        .lock()
        .expect("image memory governor poisoned")
        .clone();
    let Some(governor) = governor else {
        fail_image_load(key, request_key, epoch);
        return;
    };
    let options = governor.options();
    let budget = options.budget;
    let request = request.unwrap_or_else(|| match &key[..] {
        value if value.starts_with("url:") => crate::core::ImageRequest::new(
            crate::core::UiImageSource::url(value.trim_start_matches("url:")),
        ),
        _ => crate::core::ImageRequest::new(crate::core::UiImageSource::file(&key)),
    });
    let bytes = match crate::assets::prepare_image_bytes(bytes, &request, budget) {
        Ok(bytes) => bytes,
        Err(_) => {
            fail_image_load(key, request_key, epoch);
            return;
        }
    };
    let Some((width, height)) = image_dimensions(&bytes) else {
        fail_image_load(key, request_key, epoch);
        return;
    };

    let invalidated_request_key = request_key.clone();
    let updated = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        if !is_current_epoch(epoch) {
            false
        } else {
            let resident_bytes = bytes.len();
            cache.insert(
                key,
                CachedImage {
                    status: CachedImageStatus::Ready,
                    bytes: Some(bytes),
                    width,
                    height,
                    resident_bytes,
                    last_used: 0,
                    request_key,
                    policy: request.cache_policy_value(options.default_image_cache_policy),
                    priority: request.priority_value(),
                    reachable: true,
                },
            );
            true
        }
    };
    if updated {
        notify_image_cache_invalidated(invalidated_request_key);
    }
}

fn fail_image_load(key: String, request_key: String, epoch: u64) {
    let application_policy = application_image_cache_policy();
    let (policy, priority) = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .map_or(
            (application_policy, crate::memory::CachePriority::Normal),
            |request| {
                (
                    request.cache_policy_value(application_policy),
                    request.priority_value(),
                )
            },
        );
    let invalidated_request_key = request_key.clone();
    let updated = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        if !is_current_epoch(epoch) {
            false
        } else {
            cache.insert(
                key,
                CachedImage {
                    status: CachedImageStatus::Failed,
                    bytes: None,
                    width: 0,
                    height: 0,
                    resident_bytes: 0,
                    last_used: 0,
                    request_key,
                    policy,
                    priority,
                    reachable: true,
                },
            );
            true
        }
    };
    if updated {
        notify_image_cache_invalidated(invalidated_request_key);
    }
}

fn is_current_epoch(epoch: u64) -> bool {
    IMAGE_CACHE_EPOCH.load(Ordering::Acquire) == epoch
}

fn notify_image_cache_invalidated(request_key: String) {
    IMAGE_INVALIDATED_REQUEST_KEYS
        .lock()
        .expect("image invalidation queue poisoned")
        .insert(request_key);
    schedule_image_cache_repaint();
}

fn schedule_image_cache_repaint() {
    if IMAGE_INVALIDATED_REQUEST_KEYS
        .lock()
        .expect("image invalidation queue poisoned")
        .is_empty()
    {
        return;
    }
    if IMAGE_REPAINT_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }

    let target = *IMAGE_REPAINT_HWND
        .lock()
        .expect("image repaint hwnd poisoned");
    let posted = if let Some(hwnd) = target {
        unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut core::ffi::c_void)),
                WM_IMAGE_CACHE_INVALIDATED,
                WPARAM(0),
                LPARAM(0),
            )
            .is_ok()
        }
    } else {
        false
    };
    if !posted {
        IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
    }
}

fn image_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    let reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let (width, height) = reader.into_dimensions().ok()?;
    Some((i32::try_from(width).ok()?, i32::try_from(height).ok()?))
}

fn draw_image_bytes(
    hdc: HDC,
    rect: RECT,
    key: &str,
    bytes: &[u8],
    source_width: i32,
    source_height: i32,
    fit: ImageFit,
) -> bool {
    if source_width <= 0 || source_height <= 0 {
        return false;
    }

    DECODED_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let key = key.to_string();
        if !cache.contains_touch(&key) {
            let Some(image) = decode_image(bytes) else {
                return false;
            };
            let Some(image_bytes) = decoded_image_bytes(&image) else {
                return false;
            };
            if !cache.can_store(image_bytes) {
                return draw_decoded_image(hdc, rect, &image, source_width, source_height, fit);
            }
            cache.insert(key.clone(), image, image_bytes);
        }

        let Some(image) = cache.get(&key) else {
            return false;
        };
        if image.width != source_width || image.height != source_height || image.image.is_null() {
            return false;
        }

        draw_decoded_image(hdc, rect, image, source_width, source_height, fit)
    })
}

fn decoded_image_bytes(image: &DecodedImage) -> Option<usize> {
    usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?
        .checked_mul(4)
}

fn draw_decoded_image(
    hdc: HDC,
    rect: RECT,
    image: &DecodedImage,
    source_width: i32,
    source_height: i32,
    fit: ImageFit,
) -> bool {
    let box_width = (rect.right - rect.left).max(1);
    let box_height = (rect.bottom - rect.top).max(1);
    let scale = match fit {
        ImageFit::Cover => f32::max(
            box_width as f32 / source_width as f32,
            box_height as f32 / source_height as f32,
        ),
        ImageFit::Contain => f32::min(
            box_width as f32 / source_width as f32,
            box_height as f32 / source_height as f32,
        ),
    };
    let draw_width = ((source_width as f32 * scale).round() as i32).max(1);
    let draw_height = ((source_height as f32 * scale).round() as i32).max(1);
    let left = rect.left + (box_width - draw_width) / 2;
    let top = rect.top + (box_height - draw_height) / 2;
    unsafe {
        let mut graphics: *mut GpGraphics = std::ptr::null_mut();
        if GdipCreateFromHDC(hdc, &mut graphics) != GpOk || graphics.is_null() {
            return false;
        }
        let status = GdipDrawImageRectRectI(
            graphics,
            image.image,
            left,
            top,
            draw_width,
            draw_height,
            0,
            0,
            source_width,
            source_height,
            UnitPixel,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
        );
        let _ = GdipDeleteGraphics(graphics);
        status == GpOk
    }
}

fn decode_image(bytes: &[u8]) -> Option<DecodedImage> {
    unsafe {
        let stream = SHCreateMemStream(Some(bytes))?;
        let mut image: *mut GpImage = std::ptr::null_mut();
        if GdipLoadImageFromStream(&stream, &mut image) != GpOk || image.is_null() {
            return None;
        }

        let mut width = 0u32;
        let mut height = 0u32;
        let width_status = GdipGetImageWidth(image, &mut width);
        let height_status = GdipGetImageHeight(image, &mut height);
        if width_status != GpOk || height_status != GpOk {
            let _ = GdipDisposeImage(image);
            return None;
        }

        Some(DecodedImage {
            image,
            width: i32::try_from(width).ok()?,
            height: i32::try_from(height).ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    static IMAGE_CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    const ONE_PIXEL_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5,
        0x1C, 0x0C, 0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x64,
        0xF8, 0x0F, 0x00, 0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xE3, 0x66, 0x00, 0x00, 0x00, 0x00,
        0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
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
        while request_cached_image(&source) == CachedImageStatus::Loading
            && Instant::now() < deadline
        {
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
}
