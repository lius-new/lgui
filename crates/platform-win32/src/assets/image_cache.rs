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
static REMOTE_IMAGE_LOADER: LazyLock<Mutex<Option<lgui_core::assets::RemoteImageLoaderHandle>>> =
    LazyLock::new(|| Mutex::new(None));
static IMAGE_MEMORY_GOVERNOR: LazyLock<Mutex<Option<lgui_core::memory::MemoryGovernor>>> =
    LazyLock::new(|| Mutex::new(None));
static IMAGE_REQUESTS: LazyLock<Mutex<HashMap<String, lgui_core::core::ImageRequest>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static IMAGE_LOAD_QUEUE: LazyLock<Mutex<VecDeque<PendingImageLoad>>> =
    LazyLock::new(|| Mutex::new(VecDeque::new()));
static IMAGE_REACHABILITY: LazyLock<
    Mutex<HashMap<lgui_core::memory::DomainInstanceId, HashSet<String>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn decoded_image_telemetry() -> &'static lgui_core::memory::CacheTelemetry {
    static TELEMETRY: OnceLock<lgui_core::memory::CacheTelemetry> = OnceLock::new();
    TELEMETRY.get_or_init(Default::default)
}

thread_local! {
    static DECODED_IMAGE_CACHE: RefCell<lgui_core::memory::LruCache<String, DecodedImage>> = RefCell::new(
        lgui_core::memory::LruCache::new(
            0,
            lgui_core::memory::ResourceClass::Cache,
            decoded_image_telemetry().clone(),
        )
    );
}

pub(crate) struct RemoteImageLoaderGuard {
    previous: Option<lgui_core::assets::RemoteImageLoaderHandle>,
}

pub(crate) struct ImageMemoryGovernorGuard {
    previous: Option<lgui_core::memory::MemoryGovernor>,
}

impl Drop for ImageMemoryGovernorGuard {
    fn drop(&mut self) {
        *IMAGE_MEMORY_GOVERNOR
            .lock()
            .expect("image memory governor poisoned") = self.previous.take();
    }
}

pub(crate) fn install_image_memory_governor(
    governor: lgui_core::memory::MemoryGovernor,
) -> ImageMemoryGovernorGuard {
    let previous = IMAGE_MEMORY_GOVERNOR
        .lock()
        .expect("image memory governor poisoned")
        .replace(governor);
    ImageMemoryGovernorGuard { previous }
}

fn application_image_cache_policy() -> lgui_core::core::ImageCachePolicy {
    IMAGE_MEMORY_GOVERNOR
        .lock()
        .expect("image memory governor poisoned")
        .as_ref()
        .map(|governor| governor.options().default_image_cache_policy)
        .unwrap_or(lgui_core::core::ImageCachePolicy::NoStore)
}

impl Drop for RemoteImageLoaderGuard {
    fn drop(&mut self) {
        *REMOTE_IMAGE_LOADER
            .lock()
            .expect("remote image loader lock poisoned") = self.previous.take();
    }
}

pub(crate) fn install_remote_image_loader(
    loader: lgui_core::assets::RemoteImageLoaderHandle,
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
    bytes: Option<lgui_core::assets::AssetBytes>,
    width: i32,
    height: i32,
    resident_bytes: usize,
    last_used: u64,
    request_key: String,
    policy: lgui_core::core::ImageCachePolicy,
    priority: lgui_core::memory::CachePriority,
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

pub(crate) fn decoded_image_cache_usage() -> lgui_core::memory::CacheUsage {
    decoded_image_telemetry().snapshot()
}

pub(crate) fn trim_decoded_image_cache(target_bytes: usize) -> usize {
    DECODED_IMAGE_CACHE.with(|cache| cache.borrow_mut().trim_to(target_bytes))
}

pub(crate) fn set_decoded_image_cache_budget(budget_bytes: usize) {
    DECODED_IMAGE_CACHE.with(|cache| cache.borrow_mut().set_budget(budget_bytes));
}

fn with_cached_decoded<T>(
    key: String,
    create: impl FnOnce() -> Option<DecodedImage>,
    read: impl FnOnce(&DecodedImage) -> T,
) -> Option<T> {
    DECODED_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_touch(&key) {
            let image = create()?;
            let image_bytes = decoded_image_bytes(&image)?;
            if !cache.can_store(image_bytes) {
                return Some(read(&image));
            }
            cache.insert(key.clone(), image, image_bytes);
        }
        cache.get(&key).map(read)
    })
}

pub fn request_cached_image(source: &ImageSource) -> CachedImageStatus {
    let key = source.key();
    let request_key = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .map_or_else(|| key.clone(), lgui_core::core::ImageRequest::cache_key);
    let application_policy = application_image_cache_policy();
    let (policy, priority) = IMAGE_REQUESTS
        .lock()
        .expect("image request registry poisoned")
        .get(&key)
        .map_or(
            (application_policy, lgui_core::memory::CachePriority::Normal),
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

pub(crate) fn portable_image_cache_handle() -> lgui_core::assets::ImageCacheHandle {
    lgui_core::assets::ImageCacheHandle::new_managed(
        |request| {
            let source = match request.source() {
                lgui_core::core::UiImageSource::Url(url) => ImageSource::url(url),
                lgui_core::core::UiImageSource::File(path) => ImageSource::file(path),
                lgui_core::core::UiImageSource::Static(_)
                | lgui_core::core::UiImageSource::Bytes { .. } => {
                    return lgui_core::assets::ImageStatus::Ready
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
                lgui_core::core::UiImageSource::Url(url) => ImageSource::url(url),
                lgui_core::core::UiImageSource::File(path) => ImageSource::file(path),
                lgui_core::core::UiImageSource::Static(_)
                | lgui_core::core::UiImageSource::Bytes { .. } => return None,
            };
            cached_image_data(&source).map(|(bytes, _, _)| bytes)
        },
        || {
            let cache = IMAGE_CACHE.lock().expect("image cache poisoned");
            lgui_core::assets::ImageCacheStats {
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
                    lgui_core::core::UiImageSource::Url(url) => ImageSource::url(url).key(),
                    lgui_core::core::UiImageSource::File(path) => ImageSource::file(path).key(),
                    lgui_core::core::UiImageSource::Static(_)
                    | lgui_core::core::UiImageSource::Bytes { .. } => continue,
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
                            lgui_core::core::ImageCachePolicy::NoStore
                                | lgui_core::core::ImageCachePolicy::WhileVisible
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

fn portable_status(status: CachedImageStatus) -> lgui_core::assets::ImageStatus {
    match status {
        CachedImageStatus::Loading => lgui_core::assets::ImageStatus::Loading,
        CachedImageStatus::Ready => lgui_core::assets::ImageStatus::Ready,
        CachedImageStatus::Failed => lgui_core::assets::ImageStatus::Failed,
    }
}

pub fn cached_image_data(
    source: &ImageSource,
) -> Option<(lgui_core::assets::AssetBytes, i32, i32)> {
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
        if entry.policy == lgui_core::core::ImageCachePolicy::NoStore {
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
    with_cached_decoded(
        key,
        || {
            let (bytes, expected_width, expected_height) = cached_image_data(source)?;
            let image = decode_image(&bytes)?;
            (image.width == expected_width && image.height == expected_height).then_some(image)
        },
        |image| draw_decoded_image(hdc, rect, image, image.width, image.height, fit),
    )
    .unwrap_or(false)
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
                    .map_err(|error| lgui_core::assets::AssetError::NotFound(error.to_string()))
                    .and_then(|bytes| lgui_core::assets::validate_encoded_bytes(bytes, limit)),
                ImageSource::Url(url) => match (loader, request) {
                    (Some(loader), Some(request)) => {
                        lgui_core::assets::load_url_image(&loader, &governor, &request, &url)
                    }
                    (Some(loader), None) => loader
                        .load(&url)
                        .and_then(|bytes| lgui_core::assets::validate_encoded_bytes(bytes, limit)),
                    (None, _) => Err(lgui_core::assets::AssetError::Unsupported(
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
    bytes: lgui_core::assets::AssetBytes,
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
        value if value.starts_with("url:") => lgui_core::core::ImageRequest::new(
            lgui_core::core::UiImageSource::url(value.trim_start_matches("url:")),
        ),
        _ => lgui_core::core::ImageRequest::new(lgui_core::core::UiImageSource::file(&key)),
    });
    let bytes = match lgui_core::assets::prepare_image_bytes(bytes, &request, budget) {
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
            (application_policy, lgui_core::memory::CachePriority::Normal),
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
#[path = "image_cache_test.rs"]
mod tests;
