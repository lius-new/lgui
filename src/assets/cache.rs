use std::{cell::RefCell, sync::Arc};

#[cfg(any(test, feature = "backend-winit"))]
use std::{collections::HashMap, fs, sync::Mutex};

use super::{AssetBytes, ImageSource, ImageStatus};
#[cfg(any(test, feature = "backend-winit"))]
use super::{AssetError, RemoteImageLoaderHandle};

#[derive(Clone)]
pub struct ImageCacheHandle {
    request: Arc<dyn Fn(&ImageSource) -> ImageStatus + Send + Sync>,
    #[cfg(any(test, feature = "renderer-skia"))]
    bytes: Arc<dyn Fn(&ImageSource) -> Option<AssetBytes> + Send + Sync>,
    clear: Arc<dyn Fn() + Send + Sync>,
}

impl ImageCacheHandle {
    pub fn new(
        request: impl Fn(&ImageSource) -> ImageStatus + Send + Sync + 'static,
        bytes: impl Fn(&ImageSource) -> Option<AssetBytes> + Send + Sync + 'static,
        clear: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        #[cfg(not(any(test, feature = "renderer-skia")))]
        let _ = bytes;
        Self {
            request: Arc::new(request),
            #[cfg(any(test, feature = "renderer-skia"))]
            bytes: Arc::new(bytes),
            clear: Arc::new(clear),
        }
    }

    pub(super) fn request(&self, source: &ImageSource) -> ImageStatus {
        (self.request)(source)
    }

    fn clear(&self) {
        (self.clear)();
    }

    #[cfg(any(test, feature = "renderer-skia"))]
    pub(super) fn bytes(&self, source: &ImageSource) -> Option<AssetBytes> {
        (self.bytes)(source)
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
pub fn request_image(source: &ImageSource) -> ImageStatus {
    IMAGE_CACHE_HANDLE.with(|current| {
        current.borrow().as_ref().map_or_else(
            || match source {
                ImageSource::Static(_) | ImageSource::Bytes { .. } => ImageStatus::Ready,
                ImageSource::File(_) | ImageSource::Url(_) => ImageStatus::Failed,
            },
            |cache| cache.request(source),
        )
    })
}

/// Releases decoded and platform image caches owned by the current UI thread.
pub fn clear_image_caches() {
    IMAGE_CACHE_HANDLE.with(|current| {
        if let Some(cache) = current.borrow().as_ref() {
            cache.clear();
        }
    });
}

#[cfg(feature = "renderer-skia")]
pub(crate) fn cached_image_bytes(source: &ImageSource) -> Option<AssetBytes> {
    IMAGE_CACHE_HANDLE.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|cache| cache.bytes(source))
    })
}

#[cfg(any(test, feature = "backend-winit"))]
struct AsyncImageEntry {
    status: ImageStatus,
    bytes: Option<AssetBytes>,
    resident_bytes: usize,
    used: u64,
}

#[cfg(any(test, feature = "backend-winit"))]
struct AsyncImageState {
    entries: HashMap<String, AsyncImageEntry>,
    epoch: u64,
    generation: u64,
    resident_bytes: usize,
}

#[cfg(any(test, feature = "backend-winit"))]
impl AsyncImageState {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            epoch: 0,
            generation: 0,
            resident_bytes: 0,
        }
    }
}

#[cfg(any(test, feature = "backend-winit"))]
pub(crate) fn async_image_cache(
    loader: RemoteImageLoaderHandle,
    wake: impl Fn() + Send + Sync + 'static,
    budget_bytes: usize,
) -> ImageCacheHandle {
    let state = Arc::new(Mutex::new(AsyncImageState::new()));
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
    let request_state = Arc::clone(&state);
    let request_loader = loader.clone();
    let request_wake = Arc::clone(&wake);
    let request = move |source: &ImageSource| match source {
        ImageSource::Static(_) | ImageSource::Bytes { .. } => ImageStatus::Ready,
        ImageSource::File(_) | ImageSource::Url(_) => request_async_image(
            Arc::clone(&request_state),
            request_loader.clone(),
            Arc::clone(&request_wake),
            source.clone(),
            budget_bytes,
        ),
    };
    let bytes_state = Arc::clone(&state);
    let bytes = move |source: &ImageSource| {
        let key = image_source_key(source);
        let mut state = bytes_state.lock().expect("async image cache poisoned");
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        let entry = state.entries.get_mut(&key)?;
        entry.used = generation;
        entry.bytes.clone()
    };
    let clear = move || {
        let mut state = state.lock().expect("async image cache poisoned");
        state.epoch = state.epoch.wrapping_add(1);
        state.entries.clear();
        state.resident_bytes = 0;
    };
    ImageCacheHandle::new(request, bytes, clear)
}

#[cfg(any(test, feature = "backend-winit"))]
fn request_async_image(
    state: Arc<Mutex<AsyncImageState>>,
    loader: RemoteImageLoaderHandle,
    wake: Arc<dyn Fn() + Send + Sync>,
    source: ImageSource,
    budget_bytes: usize,
) -> ImageStatus {
    let key = image_source_key(&source);
    let epoch = {
        let mut state = state.lock().expect("async image cache poisoned");
        state.generation = state.generation.wrapping_add(1);
        let generation = state.generation;
        if let Some(entry) = state.entries.get_mut(&key) {
            entry.used = generation;
            return entry.status;
        }
        let epoch = state.epoch;
        state.entries.insert(
            key.clone(),
            AsyncImageEntry {
                status: ImageStatus::Loading,
                bytes: None,
                resident_bytes: 0,
                used: generation,
            },
        );
        epoch
    };
    let failure_state = Arc::clone(&state);
    let failure_key = key.clone();
    if std::thread::Builder::new()
        .name("lgui-image-loader".to_owned())
        .spawn(move || {
            let result = match source {
                ImageSource::File(path) => fs::read(path)
                    .map(Arc::<[u8]>::from)
                    .map_err(|error| AssetError::NotFound(error.to_string())),
                ImageSource::Url(url) => loader.load(&url),
                ImageSource::Static(_) | ImageSource::Bytes { .. } => unreachable!(),
            };
            finish_async_image(&state, &key, epoch, result, budget_bytes);
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
            budget_bytes,
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
    let (status, bytes, resident_bytes) = match result {
        Ok(bytes) => (ImageStatus::Ready, Some(bytes.clone()), bytes.len()),
        Err(_) => (ImageStatus::Failed, None, 0),
    };
    let used = state.generation;
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
        },
    );
    while state.resident_bytes > budget_bytes {
        let Some(evict) = state
            .entries
            .iter()
            .filter(|(_, entry)| entry.status != ImageStatus::Loading)
            .min_by_key(|(_, entry)| entry.used)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(entry) = state.entries.remove(&evict) {
            state.resident_bytes = state.resident_bytes.saturating_sub(entry.resident_bytes);
        }
    }
}

#[cfg(any(test, feature = "backend-winit"))]
fn image_source_key(source: &ImageSource) -> String {
    match source {
        ImageSource::Static(key) => format!("asset:{key}"),
        ImageSource::File(path) => format!("file:{}", path.display()),
        ImageSource::Url(url) => format!("url:{url}"),
        ImageSource::Bytes { key, version, .. } => format!("bytes:{key}:{version}"),
    }
}
