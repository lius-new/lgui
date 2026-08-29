use std::{
    cell::RefCell,
    collections::HashMap,
    fmt, fs,
    io::Read,
    sync::{Arc, Mutex},
};

use crate::core::{CustomPaintStyle, PhysicalSize, ScenePrimitive, UiImageSource, UiRect};

pub type ImageSource = UiImageSource;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageStatus {
    Loading,
    Ready,
    Failed,
}

#[derive(Clone)]
pub struct ImageCacheHandle {
    request: Arc<dyn Fn(&ImageSource) -> ImageStatus + Send + Sync>,
    bytes: Arc<dyn Fn(&ImageSource) -> Option<AssetBytes> + Send + Sync>,
    clear: Arc<dyn Fn() + Send + Sync>,
}

impl ImageCacheHandle {
    pub fn new(
        request: impl Fn(&ImageSource) -> ImageStatus + Send + Sync + 'static,
        bytes: impl Fn(&ImageSource) -> Option<AssetBytes> + Send + Sync + 'static,
        clear: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            request: Arc::new(request),
            bytes: Arc::new(bytes),
            clear: Arc::new(clear),
        }
    }

    fn request(&self, source: &ImageSource) -> ImageStatus {
        (self.request)(source)
    }

    fn clear(&self) {
        (self.clear)();
    }

    fn bytes(&self, source: &ImageSource) -> Option<AssetBytes> {
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

pub(crate) fn cached_image_bytes(source: &ImageSource) -> Option<AssetBytes> {
    IMAGE_CACHE_HANDLE.with(|current| {
        current
            .borrow()
            .as_ref()
            .and_then(|cache| cache.bytes(source))
    })
}

pub type AssetBytes = Arc<[u8]>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssetError {
    NotFound(String),
    InvalidData(String),
    Unsupported(String),
}

impl fmt::Display for AssetError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(formatter, "asset `{id}` was not found"),
            Self::InvalidData(message) | Self::Unsupported(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for AssetError {}

pub trait AssetResolver: Send + Sync + 'static {
    fn resolve(&self, id: &str) -> Result<AssetBytes, AssetError>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageData {
    pub size: PhysicalSize,
    pub bgra_premultiplied: AssetBytes,
}

impl ImageData {
    pub fn new(size: PhysicalSize, bytes: impl Into<AssetBytes>) -> Result<Self, AssetError> {
        let bytes = bytes.into();
        let expected = size.width.max(0) as usize * size.height.max(0) as usize * 4;
        if size.width <= 0 || size.height <= 0 || bytes.len() != expected {
            return Err(AssetError::InvalidData(format!(
                "expected {expected} BGRA bytes for {}x{}, received {}",
                size.width,
                size.height,
                bytes.len()
            )));
        }
        Ok(Self {
            size,
            bgra_premultiplied: bytes,
        })
    }
}

pub trait ImageLoader: Send + Sync + 'static {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData, AssetError>;
}

pub trait RemoteImageLoader: Send + Sync + 'static {
    fn load(&self, url: &str) -> Result<AssetBytes, AssetError>;
}

#[derive(Clone)]
pub struct RemoteImageLoaderHandle(Arc<dyn RemoteImageLoader>);

impl RemoteImageLoaderHandle {
    pub fn new(loader: impl RemoteImageLoader) -> Self {
        Self(Arc::new(loader))
    }

    fn load(&self, url: &str) -> Result<AssetBytes, AssetError> {
        self.0.load(url)
    }
}

#[cfg(feature = "renderer-skia")]
#[derive(Clone, Copy, Debug, Default)]
pub struct HttpImageLoader;

#[cfg(feature = "renderer-skia")]
impl RemoteImageLoader for HttpImageLoader {
    fn load(&self, url: &str) -> Result<AssetBytes, AssetError> {
        const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err(AssetError::Unsupported(
                "remote images must use http or https".to_owned(),
            ));
        }
        let response = ureq::get(url)
            .call()
            .map_err(|error| AssetError::NotFound(error.to_string()))?;
        let mut bytes = Vec::new();
        response
            .into_reader()
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|error| AssetError::InvalidData(error.to_string()))?;
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(AssetError::InvalidData(
                "remote image exceeds the 32 MiB limit".to_owned(),
            ));
        }
        Ok(Arc::from(bytes))
    }
}

#[cfg(feature = "renderer-skia")]
pub fn http_image_loader() -> RemoteImageLoaderHandle {
    RemoteImageLoaderHandle::new(HttpImageLoader)
}

struct AsyncImageEntry {
    status: ImageStatus,
    bytes: Option<AssetBytes>,
    resident_bytes: usize,
    used: u64,
}

struct AsyncImageState {
    entries: HashMap<String, AsyncImageEntry>,
    epoch: u64,
    generation: u64,
    resident_bytes: usize,
}

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
        ImageSource::File(_) | ImageSource::Url(_) => {
            request_async_image(
                Arc::clone(&request_state),
                request_loader.clone(),
                Arc::clone(&request_wake),
                source.clone(),
                budget_bytes,
            )
        }
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

fn image_source_key(source: &ImageSource) -> String {
    match source {
        ImageSource::Static(key) => format!("asset:{key}"),
        ImageSource::File(path) => format!("file:{}", path.display()),
        ImageSource::Url(url) => format!("url:{url}"),
        ImageSource::Bytes { key, version, .. } => format!("bytes:{key}:{version}"),
    }
}

pub trait CustomPaintProvider: Send + Sync + 'static {
    fn record(
        &self,
        key: &str,
        bounds: UiRect,
        style: CustomPaintStyle,
    ) -> Result<Option<SceneFragment>, AssetError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneFragment {
    commands: Arc<[ScenePrimitive]>,
}

impl SceneFragment {
    pub fn new(commands: impl Into<Arc<[ScenePrimitive]>>) -> Self {
        Self {
            commands: commands.into(),
        }
    }

    pub fn commands(&self) -> &[ScenePrimitive] {
        &self.commands
    }
}

#[cfg(feature = "svg")]
pub trait SvgRenderer: Send + Sync + 'static {
    fn render(&self, source: &[u8], size: PhysicalSize) -> Result<ImageData, AssetError>;
}

#[derive(Clone, Default)]
pub struct RenderResources {
    resolver: Option<Arc<dyn AssetResolver>>,
    image_loader: Option<Arc<dyn ImageLoader>>,
    custom_paint: Option<Arc<dyn CustomPaintProvider>>,
    #[cfg(feature = "svg")]
    svg_renderer: Option<Arc<dyn SvgRenderer>>,
}

thread_local! {
    static CURRENT_RENDER_RESOURCES: RefCell<Vec<RenderResources>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn with_render_resources<R>(
    resources: RenderResources,
    use_resources: impl FnOnce() -> R,
) -> R {
    CURRENT_RENDER_RESOURCES.with(|current| current.borrow_mut().push(resources));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT_RENDER_RESOURCES.with(|current| {
                current.borrow_mut().pop();
            });
        }
    }
    let _reset = Reset;
    use_resources()
}

pub(crate) fn render_resources() -> RenderResources {
    CURRENT_RENDER_RESOURCES.with(|current| current.borrow().last().cloned().unwrap_or_default())
}

impl RenderResources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_resolver(mut self, resolver: impl AssetResolver) -> Self {
        self.resolver = Some(Arc::new(resolver));
        self
    }

    pub fn with_image_loader(mut self, loader: impl ImageLoader) -> Self {
        self.image_loader = Some(Arc::new(loader));
        self
    }

    pub fn with_custom_paint(mut self, provider: impl CustomPaintProvider) -> Self {
        self.custom_paint = Some(Arc::new(provider));
        self
    }

    #[cfg(feature = "svg")]
    pub fn with_svg_renderer(mut self, renderer: impl SvgRenderer) -> Self {
        self.svg_renderer = Some(Arc::new(renderer));
        self
    }

    pub fn resolver(&self) -> Option<&Arc<dyn AssetResolver>> {
        self.resolver.as_ref()
    }

    pub fn image_loader(&self) -> Option<&Arc<dyn ImageLoader>> {
        self.image_loader.as_ref()
    }

    pub fn custom_paint(&self) -> Option<&Arc<dyn CustomPaintProvider>> {
        self.custom_paint.as_ref()
    }

    #[cfg(feature = "svg")]
    pub fn svg_renderer(&self) -> Option<&Arc<dyn SvgRenderer>> {
        self.svg_renderer.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{sync::mpsc, time::Duration};

    struct TestRemoteLoader;

    impl RemoteImageLoader for TestRemoteLoader {
        fn load(&self, _url: &str) -> Result<AssetBytes, AssetError> {
            Ok(Arc::from([1_u8, 2, 3, 4]))
        }
    }

    #[test]
    fn image_data_rejects_invalid_stride_length() {
        let error = ImageData::new(PhysicalSize::new(2, 2), Arc::<[u8]>::from([0_u8; 15]))
            .expect_err("invalid BGRA length must fail");
        assert!(matches!(error, AssetError::InvalidData(_)));
    }

    #[test]
    fn image_cache_capability_is_scoped_and_restored() {
        let source = ImageSource::url("https://example.invalid/image.png");
        assert_eq!(request_image(&source), ImageStatus::Failed);
        {
            let _guard =
                install_image_cache(ImageCacheHandle::new(
                    |_| ImageStatus::Loading,
                    |_| None,
                    || {},
                ));
            assert_eq!(request_image(&source), ImageStatus::Loading);
        }
        assert_eq!(request_image(&source), ImageStatus::Failed);
    }

    #[test]
    fn async_image_cache_wakes_and_publishes_loaded_bytes() {
        let (wake, woke) = mpsc::channel();
        let cache = async_image_cache(
            RemoteImageLoaderHandle::new(TestRemoteLoader),
            move || {
                let _ = wake.send(());
            },
            1024,
        );
        let source = ImageSource::url("https://example.invalid/test.png");
        assert_eq!(cache.request(&source), ImageStatus::Loading);
        woke.recv_timeout(Duration::from_secs(2))
            .expect("image completion wake");
        assert_eq!(cache.request(&source), ImageStatus::Ready);
        assert_eq!(cache.bytes(&source).as_deref(), Some(&[1, 2, 3, 4][..]));
    }
}
