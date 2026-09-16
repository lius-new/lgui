#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
use std::io::Cursor;
use std::{cell::RefCell, sync::Arc};

#[cfg(any(test, feature = "backend-winit"))]
use std::{
    collections::HashMap,
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

#[cfg(feature = "persistent-cache")]
use std::time::{SystemTime, UNIX_EPOCH};

use super::{AssetBytes, ImageSource, ImageStatus};
#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
use super::{AssetError, RemoteImageLoaderHandle};
#[cfg(any(
    test,
    feature = "persistent-cache",
    all(feature = "backend-winit", feature = "images")
))]
use crate::core::ImageCachePolicy;
#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
use crate::core::ImageDecodePolicy;
use crate::core::ImageRequest;

#[derive(Clone)]
pub struct ImageCacheHandle {
    request: Arc<dyn Fn(&ImageRequest) -> ImageStatus + Send + Sync>,
    #[cfg(any(test, feature = "renderer-skia"))]
    bytes: Arc<dyn Fn(&ImageRequest) -> Option<AssetBytes> + Send + Sync>,
    stats: Arc<dyn Fn() -> ImageCacheStats + Send + Sync>,
    trim: Arc<dyn Fn(usize) -> usize + Send + Sync>,
    set_budget: Arc<dyn Fn(usize) + Send + Sync>,
    reachability: Arc<dyn Fn(crate::memory::DomainInstanceId, &[ImageRequest]) + Send + Sync>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ImageCacheStats {
    pub entries: usize,
    pub resident_bytes: usize,
    pub pinned_bytes: usize,
    pub budget_bytes: usize,
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub largest_entry_bytes: usize,
    pub in_flight: usize,
}

impl ImageCacheHandle {
    pub fn new(
        request: impl Fn(&ImageRequest) -> ImageStatus + Send + Sync + 'static,
        bytes: impl Fn(&ImageRequest) -> Option<AssetBytes> + Send + Sync + 'static,
    ) -> Self {
        Self::new_managed(
            request,
            bytes,
            ImageCacheStats::default,
            |_| 0,
            |_| {},
            |_, _| {},
        )
    }

    #[doc(hidden)]
    pub fn new_managed(
        request: impl Fn(&ImageRequest) -> ImageStatus + Send + Sync + 'static,
        bytes: impl Fn(&ImageRequest) -> Option<AssetBytes> + Send + Sync + 'static,
        stats: impl Fn() -> ImageCacheStats + Send + Sync + 'static,
        trim: impl Fn(usize) -> usize + Send + Sync + 'static,
        set_budget: impl Fn(usize) + Send + Sync + 'static,
        reachability: impl Fn(crate::memory::DomainInstanceId, &[ImageRequest]) + Send + Sync + 'static,
    ) -> Self {
        #[cfg(not(any(test, feature = "renderer-skia")))]
        let _ = bytes;
        Self {
            request: Arc::new(request),
            #[cfg(any(test, feature = "renderer-skia"))]
            bytes: Arc::new(bytes),
            stats: Arc::new(stats),
            trim: Arc::new(trim),
            set_budget: Arc::new(set_budget),
            reachability: Arc::new(reachability),
        }
    }

    pub(super) fn request(&self, request: &ImageRequest) -> ImageStatus {
        (self.request)(request)
    }

    pub fn stats(&self) -> ImageCacheStats {
        (self.stats)()
    }

    pub fn trim_to(&self, target_bytes: usize) -> usize {
        (self.trim)(target_bytes)
    }

    pub fn set_budget(&self, budget_bytes: usize) {
        (self.set_budget)(budget_bytes);
    }

    pub fn update_reachability(
        &self,
        owner: crate::memory::DomainInstanceId,
        requests: &[ImageRequest],
    ) {
        (self.reachability)(owner, requests);
    }

    #[cfg(any(test, feature = "renderer-skia"))]
    pub(super) fn bytes(&self, request: &ImageRequest) -> Option<AssetBytes> {
        (self.bytes)(request)
    }
}

thread_local! {
    static IMAGE_CACHE_HANDLE: RefCell<Option<ImageCacheHandle>> = const { RefCell::new(None) };
}

pub(crate) struct ImageCacheGuard {
    previous: Option<ImageCacheHandle>,
}

impl Drop for ImageCacheGuard {
    fn drop(&mut self) {
        IMAGE_CACHE_HANDLE.with(|current| {
            *current.borrow_mut() = self.previous.take();
        });
    }
}

pub(crate) fn install_image_cache(handle: ImageCacheHandle) -> ImageCacheGuard {
    let previous = IMAGE_CACHE_HANDLE.with(|current| current.borrow_mut().replace(handle));
    ImageCacheGuard { previous }
}

/// Starts loading an image and returns its current cache status.
pub fn request_image(request: &ImageRequest) -> ImageStatus {
    IMAGE_CACHE_HANDLE.with(|current| {
        current.borrow().as_ref().map_or_else(
            || match request.source() {
                ImageSource::Static(_) | ImageSource::Bytes { .. } => ImageStatus::Ready,
                ImageSource::File(_) | ImageSource::Url(_) => ImageStatus::Failed,
            },
            |cache| cache.request(request),
        )
    })
}

pub(crate) fn update_image_reachability(
    owner: crate::memory::DomainInstanceId,
    requests: &[ImageRequest],
) {
    IMAGE_CACHE_HANDLE.with(|current| {
        if let Some(cache) = current.borrow().as_ref() {
            cache.update_reachability(owner, requests);
        }
    });
}

#[cfg(feature = "renderer-skia")]
pub(crate) fn cached_image_bytes(request: &ImageRequest) -> Option<AssetBytes> {
    IMAGE_CACHE_HANDLE.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|cache| cache.bytes(request))
    })
}

#[cfg(any(test, feature = "backend-winit"))]
struct AsyncImageEntry {
    status: ImageStatus,
    bytes: Option<AssetBytes>,
    resident_bytes: usize,
    used: u64,
    retry_at: Option<Instant>,
    policy: ImageCachePolicy,
    priority: crate::memory::CachePriority,
    reachable: bool,
}

#[cfg(any(test, feature = "backend-winit"))]
struct AsyncImageState {
    entries: HashMap<String, AsyncImageEntry>,
    reachable_by_owner: HashMap<crate::memory::DomainInstanceId, std::collections::HashSet<String>>,
    epoch: u64,
    generation: u64,
    resident_bytes: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

#[cfg(any(test, feature = "backend-winit"))]
impl AsyncImageState {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            reachable_by_owner: HashMap::new(),
            epoch: 0,
            generation: 0,
            resident_bytes: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }
}

#[cfg(any(test, feature = "backend-winit"))]
pub(crate) fn async_image_cache(
    loader: RemoteImageLoaderHandle,
    wake: impl Fn() + Send + Sync + 'static,
    budget_bytes: usize,
    governor: crate::memory::MemoryGovernor,
) -> ImageCacheHandle {
    let state = Arc::new(Mutex::new(AsyncImageState::new()));
    let budget = Arc::new(AtomicUsize::new(budget_bytes));
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    let request_state = Arc::clone(&state);
    let request_loader = loader.clone();
    let request_wake = Arc::clone(&wake);
    let request_budget = Arc::clone(&budget);
    let request_governor = governor.clone();
    let request = move |request: &ImageRequest| match request.source() {
        ImageSource::Static(_) | ImageSource::Bytes { .. } => ImageStatus::Ready,
        ImageSource::File(_) | ImageSource::Url(_) => request_async_image(
            Arc::clone(&request_state),
            request_loader.clone(),
            Arc::clone(&request_wake),
            request.clone(),
            Arc::clone(&request_budget),
            request_governor.clone(),
        ),
    };
    let bytes_state = Arc::clone(&state);
    let bytes_governor = governor.clone();
    let bytes = move |request: &ImageRequest| {
        let key = request.cache_key();
        let mut state = bytes_state.lock().expect("async image cache poisoned");
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        let entry = state.entries.get_mut(&key)?;
        entry.used = generation;
        entry.reachable = true;
        let bytes = entry.bytes.clone()?;
        let remove_after_read = entry.policy == ImageCachePolicy::NoStore;
        if remove_after_read {
            if let Some(entry) = state.entries.remove(&key) {
                state.resident_bytes = state.resident_bytes.saturating_sub(entry.resident_bytes);
                state.evictions = state.evictions.saturating_add(1);
            }
        }
        drop(state);
        let budget = bytes_governor.options().budget;
        let _reservation = bytes_governor.try_reserve_task(
            budget
                .max_decoded_resource_bytes
                .min(budget.transient_hard_bytes),
        )?;
        prepare_image_bytes(bytes, request, budget).ok()
    };
    let stats_state = Arc::clone(&state);
    let stats_budget = Arc::clone(&budget);
    let stats = move || {
        let state = stats_state.lock().expect("async image cache poisoned");
        ImageCacheStats {
            entries: state.entries.len(),
            resident_bytes: state.resident_bytes,
            pinned_bytes: state
                .entries
                .values()
                .filter(|entry| entry.reachable)
                .map(|entry| entry.resident_bytes)
                .sum(),
            budget_bytes: stats_budget.load(Ordering::Acquire),
            hits: state.hits,
            misses: state.misses,
            evictions: state.evictions,
            largest_entry_bytes: state
                .entries
                .values()
                .map(|entry| entry.resident_bytes)
                .max()
                .unwrap_or(0),
            in_flight: state
                .entries
                .values()
                .filter(|entry| entry.status == ImageStatus::Loading)
                .count(),
        }
    };
    let trim_state = Arc::clone(&state);
    let trim = move |target_bytes| {
        let mut state = trim_state.lock().expect("async image cache poisoned");
        let before = state.resident_bytes;
        if target_bytes == 0 {
            state.epoch = state.epoch.wrapping_add(1);
            state.reachable_by_owner.clear();
        }
        evict_async_images(&mut state, target_bytes, 0, target_bytes != 0);
        before.saturating_sub(state.resident_bytes)
    };
    let set_state = Arc::clone(&state);
    let set_budget_value = Arc::clone(&budget);
    let set_budget = move |value: usize| {
        set_budget_value.store(value, Ordering::Release);
        let mut state = set_state.lock().expect("async image cache poisoned");
        evict_async_images(&mut state, value, 4096, true);
    };
    let reachability_state = Arc::clone(&state);
    let reachability_governor = governor.clone();
    let reachability_budget = Arc::clone(&budget);
    let reachability = move |owner, requests: &[ImageRequest]| {
        let mut state = reachability_state
            .lock()
            .expect("async image cache poisoned");
        if requests.is_empty() {
            state.reachable_by_owner.remove(&owner);
        } else {
            state.reachable_by_owner.insert(
                owner,
                requests.iter().map(ImageRequest::cache_key).collect(),
            );
        }
        let reachable = state
            .reachable_by_owner
            .values()
            .flatten()
            .cloned()
            .collect::<std::collections::HashSet<_>>();
        for entry in state.entries.values_mut() {
            entry.reachable = false;
        }
        for request in requests {
            if let Some(entry) = state.entries.get_mut(&request.cache_key()) {
                entry.policy = request
                    .cache_policy_value(reachability_governor.options().default_image_cache_policy);
                entry.priority = request.priority_value();
            }
        }
        for (key, entry) in &mut state.entries {
            entry.reachable = reachable.contains(key);
        }
        evict_unreachable_visible_images(&mut state);
        evict_async_images(
            &mut state,
            reachability_budget.load(Ordering::Acquire),
            4096,
            true,
        );
    };
    ImageCacheHandle::new_managed(request, bytes, stats, trim, set_budget, reachability)
}

#[cfg(any(test, feature = "backend-winit"))]
fn request_async_image(
    state: Arc<Mutex<AsyncImageState>>,
    loader: RemoteImageLoaderHandle,
    wake: Arc<dyn Fn() + Send + Sync>,
    request: ImageRequest,
    budget_bytes: Arc<AtomicUsize>,
    governor: crate::memory::MemoryGovernor,
) -> ImageStatus {
    let key = request.cache_key();
    let epoch = {
        let mut state = state.lock().expect("async image cache poisoned");
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        if let Some(entry) = state.entries.get_mut(&key) {
            if entry.status == ImageStatus::Failed
                && entry
                    .retry_at
                    .is_some_and(|retry_at| Instant::now() >= retry_at)
            {
                state.entries.remove(&key);
            } else {
                entry.used = generation;
                let status = entry.status;
                let _ = entry;
                state.hits = state.hits.saturating_add(1);
                return status;
            }
        }
        state.misses = state.misses.saturating_add(1);
        let epoch = state.epoch;
        state.entries.insert(
            key.clone(),
            AsyncImageEntry {
                status: ImageStatus::Loading,
                bytes: None,
                resident_bytes: 0,
                used: generation,
                retry_at: None,
                policy: request.cache_policy_value(governor.options().default_image_cache_policy),
                priority: request.priority_value(),
                reachable: true,
            },
        );
        epoch
    };
    let reservation_bytes = governor.options().budget.max_encoded_resource_bytes;
    let Some(task_reservation) = governor.try_reserve_task(reservation_bytes) else {
        finish_async_image(
            &state,
            &key,
            epoch,
            Err(AssetError::Unsupported(
                "image load deferred by memory pressure".to_owned(),
            )),
            budget_bytes.load(Ordering::Acquire),
        );
        return ImageStatus::Failed;
    };
    let failure_state = Arc::clone(&state);
    let failure_key = key.clone();
    let worker_budget = Arc::clone(&budget_bytes);
    if std::thread::Builder::new()
        .name("lgui-image-loader".to_owned())
        .spawn(move || {
            let _task_reservation = task_reservation;
            let result = match request.source() {
                ImageSource::File(path) => fs::read(path)
                    .map(Arc::<[u8]>::from)
                    .map_err(|error| AssetError::NotFound(error.to_string())),
                ImageSource::Url(url) => load_url_image(&loader, &governor, &request, url),
                ImageSource::Static(_) | ImageSource::Bytes { .. } => unreachable!(),
            };
            finish_async_image(
                &state,
                &key,
                epoch,
                result,
                worker_budget.load(Ordering::Acquire),
            );
            wake();
        })
        .is_err()
    {
        finish_async_image(
            &failure_state,
            &failure_key,
            epoch,
            Err(AssetError::Unsupported(
                "could not start image loading thread".to_owned(),
            )),
            budget_bytes.load(Ordering::Acquire),
        );
    }
    ImageStatus::Loading
}

#[cfg(any(test, feature = "backend-winit"))]
fn finish_async_image(
    state: &Mutex<AsyncImageState>,
    key: &str,
    epoch: u64,
    result: Result<AssetBytes, AssetError>,
    budget_bytes: usize,
) {
    let mut state = state.lock().expect("async image cache poisoned");
    if state.epoch != epoch {
        return;
    }
    let (status, bytes, resident_bytes, retry_at) = match result {
        Ok(bytes) => (ImageStatus::Ready, Some(bytes.clone()), bytes.len(), None),
        Err(_) => (
            ImageStatus::Failed,
            None,
            0,
            Some(Instant::now() + Duration::from_secs(5)),
        ),
    };
    let used = state.generation;
    let (policy, priority, reachable) = state.entries.get(key).map_or(
        (
            ImageCachePolicy::NoStore,
            crate::memory::CachePriority::Normal,
            true,
        ),
        |entry| (entry.policy, entry.priority, entry.reachable),
    );
    if let Some(previous) = state.entries.remove(key) {
        state.resident_bytes = state.resident_bytes.saturating_sub(previous.resident_bytes);
    }
    state.resident_bytes = state.resident_bytes.saturating_add(resident_bytes);
    state.entries.insert(
        key.to_owned(),
        AsyncImageEntry {
            status,
            bytes,
            resident_bytes,
            used,
            retry_at,
            policy,
            priority,
            reachable,
        },
    );
    evict_async_images(&mut state, budget_bytes, 4096, true);
}

#[doc(hidden)]
#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
pub fn load_url_image(
    loader: &RemoteImageLoaderHandle,
    governor: &crate::memory::MemoryGovernor,
    request: &ImageRequest,
    url: &str,
) -> Result<AssetBytes, AssetError> {
    #[cfg(not(feature = "persistent-cache"))]
    let _ = request;
    let limit = governor.options().budget.max_encoded_resource_bytes;
    #[cfg(feature = "persistent-cache")]
    if let ImageCachePolicy::Persistent {
        max_age,
        revalidate,
    } = request.cache_policy_value(governor.options().default_image_cache_policy)
    {
        if let Some(store) = governor.persistent_cache() {
            let key = crate::memory::PersistentCacheKey::new(
                request.namespace_value(),
                url,
                request.version_value(),
            );
            let stale = store.get_stale(&key).ok().flatten();
            if let Some(entry) = stale.as_ref().filter(|entry| !entry.is_expired()) {
                return validate_encoded_bytes(Arc::from(entry.bytes.clone()), limit);
            }
            let response = loader.load_response(
                url,
                revalidate
                    .then(|| stale.as_ref()?.etag.as_deref())
                    .flatten(),
                revalidate
                    .then(|| stale.as_ref()?.last_modified.as_deref())
                    .flatten(),
            );
            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    if let Some(entry) = stale {
                        return validate_encoded_bytes(Arc::from(entry.bytes), limit);
                    }
                    return Err(error);
                }
            };
            let bytes = if response.not_modified {
                stale
                    .as_ref()
                    .map(|entry| Arc::<[u8]>::from(entry.bytes.clone()))
                    .ok_or_else(|| {
                        AssetError::InvalidData(
                            "image server returned 304 without a cached response".to_owned(),
                        )
                    })?
            } else {
                response.bytes.clone().ok_or_else(|| {
                    AssetError::InvalidData("image response had no body".to_owned())
                })?
            };
            let bytes = validate_encoded_bytes(bytes, limit)?;
            if !request.is_sensitive()
                && response
                    .mime
                    .as_deref()
                    .is_none_or(|mime| mime.starts_with("image/"))
            {
                let mut entry = crate::memory::PersistentEntry::new(key, bytes.to_vec());
                entry.mime = response
                    .mime
                    .or_else(|| stale.as_ref().and_then(|entry| entry.mime.clone()));
                entry.etag = response
                    .etag
                    .or_else(|| stale.as_ref().and_then(|entry| entry.etag.clone()));
                entry.last_modified = response
                    .last_modified
                    .or_else(|| stale.as_ref().and_then(|entry| entry.last_modified.clone()));
                entry.expires_unix_seconds = Some(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs()
                        .saturating_add(max_age.as_secs()),
                );
                let _ = store.put(entry);
                let _ = store.trim_to(governor.options().budget.persistent_bytes);
            }
            return Ok(bytes);
        }
    }
    validate_encoded_bytes(loader.load(url)?, limit)
}

#[doc(hidden)]
#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
pub fn validate_encoded_bytes(bytes: AssetBytes, limit: usize) -> Result<AssetBytes, AssetError> {
    if bytes.len() > limit {
        return Err(AssetError::InvalidData(format!(
            "encoded image exceeds the {} byte limit",
            limit
        )));
    }
    Ok(bytes)
}

#[doc(hidden)]
#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
pub fn prepare_image_bytes(
    bytes: AssetBytes,
    request: &ImageRequest,
    budget: crate::memory::MemoryBudget,
) -> Result<AssetBytes, AssetError> {
    let bytes = validate_encoded_bytes(bytes, budget.max_encoded_resource_bytes)?;
    let reader = image::ImageReader::new(Cursor::new(bytes.as_ref()))
        .with_guessed_format()
        .map_err(|error| AssetError::InvalidData(error.to_string()))?;
    let (width, height) = reader
        .into_dimensions()
        .map_err(|error| AssetError::InvalidData(error.to_string()))?;
    validate_decoded_dimensions(width, height, budget.max_decoded_resource_bytes)?;

    let ImageDecodePolicy::FitTarget(target) = request.decode_policy_value() else {
        return Ok(bytes);
    };
    let target_width = u32::try_from(target.width).ok().filter(|value| *value > 0);
    let target_height = u32::try_from(target.height).ok().filter(|value| *value > 0);
    let (Some(target_width), Some(target_height)) = (target_width, target_height) else {
        return Err(AssetError::InvalidData(
            "image decode target must be positive".to_owned(),
        ));
    };
    validate_decoded_dimensions(
        target_width,
        target_height,
        budget.max_decoded_resource_bytes,
    )?;
    if width <= target_width && height <= target_height {
        return Ok(bytes);
    }

    let decoded = image::load_from_memory(bytes.as_ref())
        .map_err(|error| AssetError::InvalidData(error.to_string()))?;
    let resized = decoded.thumbnail(target_width, target_height);
    let mut encoded = Cursor::new(Vec::new());
    resized
        .write_to(&mut encoded, image::ImageFormat::Png)
        .map_err(|error| AssetError::InvalidData(error.to_string()))?;
    validate_encoded_bytes(
        Arc::from(encoded.into_inner()),
        budget.max_encoded_resource_bytes,
    )
}

#[cfg(any(test, feature = "backend-winit", feature = "images-win32"))]
fn validate_decoded_dimensions(
    width: u32,
    height: u32,
    max_bytes: usize,
) -> Result<(), AssetError> {
    let bytes = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| AssetError::InvalidData("decoded image dimensions overflow".to_owned()))?;
    if width == 0 || height == 0 || bytes > max_bytes {
        return Err(AssetError::InvalidData(format!(
            "decoded image {}x{} exceeds the {} byte limit",
            width, height, max_bytes
        )));
    }
    Ok(())
}

#[cfg(any(test, feature = "backend-winit"))]
fn evict_async_images(
    state: &mut AsyncImageState,
    budget_bytes: usize,
    max_entries: usize,
    preserve_reachable: bool,
) {
    while state.resident_bytes > budget_bytes || state.entries.len() > max_entries {
        let Some(evict) = state
            .entries
            .iter()
            .filter(|(_, entry)| {
                entry.status != ImageStatus::Loading && (!preserve_reachable || !entry.reachable)
            })
            .min_by_key(|(_, entry)| (entry.reachable, entry.priority, entry.used))
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(entry) = state.entries.remove(&evict) {
            state.resident_bytes = state.resident_bytes.saturating_sub(entry.resident_bytes);
            state.evictions = state.evictions.saturating_add(1);
        }
    }
}

#[cfg(any(test, feature = "backend-winit"))]
fn evict_unreachable_visible_images(state: &mut AsyncImageState) {
    let evict = state
        .entries
        .iter()
        .filter_map(|(key, entry)| {
            (!entry.reachable
                && entry.status != ImageStatus::Loading
                && matches!(
                    entry.policy,
                    ImageCachePolicy::NoStore | ImageCachePolicy::WhileVisible
                ))
            .then(|| key.clone())
        })
        .collect::<Vec<_>>();
    for key in evict {
        if let Some(entry) = state.entries.remove(&key) {
            state.resident_bytes = state.resident_bytes.saturating_sub(entry.resident_bytes);
            state.evictions = state.evictions.saturating_add(1);
        }
    }
}
