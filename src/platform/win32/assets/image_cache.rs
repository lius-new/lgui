use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, LazyLock, Mutex,
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

static IMAGE_CACHE: LazyLock<Mutex<ImageCache>> =
    LazyLock::new(|| Mutex::new(ImageCache::default()));
static IMAGE_CACHE_EPOCH: AtomicU64 = AtomicU64::new(0);
static IMAGE_REPAINT_HWND: LazyLock<Mutex<Option<isize>>> = LazyLock::new(|| Mutex::new(None));
static IMAGE_REPAINT_PENDING: AtomicBool = AtomicBool::new(false);
static REMOTE_IMAGE_LOADER: LazyLock<Mutex<Option<crate::assets::RemoteImageLoaderHandle>>> =
    LazyLock::new(|| Mutex::new(None));

thread_local! {
    static DECODED_IMAGE_CACHE: RefCell<HashMap<String, DecodedImage>> = RefCell::new(HashMap::new());
}

pub(crate) struct RemoteImageLoaderGuard {
    previous: Option<crate::assets::RemoteImageLoaderHandle>,
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

#[derive(Default)]
struct ImageCache {
    entries: HashMap<String, CachedImage>,
}

struct CachedImage {
    status: CachedImageStatus,
    bytes: Option<Vec<u8>>,
    width: i32,
    height: i32,
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
    let mut target = IMAGE_REPAINT_HWND
        .lock()
        .expect("image repaint hwnd poisoned");
    *target = Some(hwnd.0 as isize);
}

pub fn clear_image_repaint_hwnd(hwnd: HWND) {
    let mut target = IMAGE_REPAINT_HWND
        .lock()
        .expect("image repaint hwnd poisoned");
    if *target == Some(hwnd.0 as isize) {
        *target = None;
    }
}

pub fn mark_image_cache_repaint_handled() {
    IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
}

pub fn clear_decoded_image_cache() {
    DECODED_IMAGE_CACHE.with(|cache| {
        cache.borrow_mut().clear();
    });
}

pub fn clear_cached_image_cache() {
    IMAGE_CACHE_EPOCH.fetch_add(1, Ordering::AcqRel);
    IMAGE_CACHE
        .lock()
        .expect("image cache poisoned")
        .entries
        .clear();
    IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
}

pub fn request_cached_image(source: &ImageSource) -> CachedImageStatus {
    let key = source.key();
    let epoch = IMAGE_CACHE_EPOCH.load(Ordering::Acquire);
    let mut should_start = false;
    let status = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        match cache.entries.get(&key) {
            Some(entry) => entry.status,
            None => {
                cache.entries.insert(
                    key.clone(),
                    CachedImage {
                        status: CachedImageStatus::Loading,
                        bytes: None,
                        width: 0,
                        height: 0,
                    },
                );
                should_start = true;
                CachedImageStatus::Loading
            }
        }
    };

    if should_start {
        start_image_load(key, source.clone(), epoch);
    }

    status
}

pub(crate) fn portable_image_cache_handle() -> crate::assets::ImageCacheHandle {
    crate::assets::ImageCacheHandle::new(
        |source| match source {
            crate::core::UiImageSource::Url(url) => {
                portable_status(request_cached_image(&ImageSource::url(url)))
            }
            crate::core::UiImageSource::File(path) => {
                portable_status(request_cached_image(&ImageSource::file(path)))
            }
            crate::core::UiImageSource::Static(_) | crate::core::UiImageSource::Bytes { .. } => {
                crate::assets::ImageStatus::Ready
            }
        },
        |source| {
            let source = match source {
                crate::core::UiImageSource::Url(url) => ImageSource::url(url),
                crate::core::UiImageSource::File(path) => ImageSource::file(path),
                crate::core::UiImageSource::Static(_)
                | crate::core::UiImageSource::Bytes { .. } => return None,
            };
            cached_image_data(&source).map(|(bytes, _, _)| Arc::<[u8]>::from(bytes))
        },
        || {
            clear_cached_image_cache();
            clear_decoded_image_cache();
            #[cfg(feature = "advanced-rendering")]
            super::enhanced::image::clear_decoded_image_cache();
        },
    )
}

fn portable_status(status: CachedImageStatus) -> crate::assets::ImageStatus {
    match status {
        CachedImageStatus::Loading => crate::assets::ImageStatus::Loading,
        CachedImageStatus::Ready => crate::assets::ImageStatus::Ready,
        CachedImageStatus::Failed => crate::assets::ImageStatus::Failed,
    }
}

pub fn cached_image_data(source: &ImageSource) -> Option<(Vec<u8>, i32, i32)> {
    let key = source.key();
    let entry = {
        let cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        let Some(entry) = cache.entries.get(&key) else {
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
        (bytes.clone(), entry.width, entry.height)
    };
    Some(entry)
}

pub fn draw_cached_image(hdc: HDC, rect: RECT, source: &ImageSource, fit: ImageFit) -> bool {
    let key = source.key();
    let entry = {
        let cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        let Some(entry) = cache.entries.get(&key) else {
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
        (bytes.clone(), entry.width, entry.height)
    };

    draw_image_bytes(hdc, rect, &key, &entry.0, entry.1, entry.2, fit)
}

fn start_image_load(key: String, source: ImageSource, epoch: u64) {
    match source {
        ImageSource::Asset { bytes, .. } => {
            finish_image_load(key, bytes.to_vec(), epoch);
        }
        ImageSource::File(path) => {
            let failure_key = key.clone();
            if std::thread::Builder::new()
                .name("lgui-image-file".to_string())
                .spawn(move || match fs::read(path) {
                    Ok(bytes) => finish_image_load(key, bytes, epoch),
                    Err(_) => fail_image_load(key, epoch),
                })
                .is_err()
            {
                fail_image_load(failure_key, epoch);
            }
        }
        ImageSource::Url(url) => {
            let Some(loader) = REMOTE_IMAGE_LOADER
                .lock()
                .expect("remote image loader lock poisoned")
                .clone()
            else {
                fail_image_load(key, epoch);
                return;
            };
            let failure_key = key.clone();
            if std::thread::Builder::new()
                .name("lgui-image-http".to_owned())
                .spawn(move || match loader.load(&url) {
                    Ok(bytes) => finish_image_load(key, bytes.to_vec(), epoch),
                    Err(_) => fail_image_load(key, epoch),
                })
                .is_err()
            {
                fail_image_load(failure_key, epoch);
            }
        }
    }
}

fn finish_image_load(key: String, bytes: Vec<u8>, epoch: u64) {
    if !is_current_epoch(epoch) {
        return;
    }
    let Some((width, height)) = image_dimensions(&bytes) else {
        fail_image_load(key, epoch);
        return;
    };

    let updated = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        if !is_current_epoch(epoch) {
            false
        } else {
            cache.entries.insert(
                key,
                CachedImage {
                    status: CachedImageStatus::Ready,
                    bytes: Some(bytes),
                    width,
                    height,
                },
            );
            true
        }
    };
    if updated {
        notify_image_cache_invalidated();
    }
}

fn fail_image_load(key: String, epoch: u64) {
    let updated = {
        let mut cache = IMAGE_CACHE.lock().expect("image cache poisoned");
        if !is_current_epoch(epoch) {
            false
        } else {
            cache.entries.insert(
                key,
                CachedImage {
                    status: CachedImageStatus::Failed,
                    bytes: None,
                    width: 0,
                    height: 0,
                },
            );
            true
        }
    };
    if updated {
        notify_image_cache_invalidated();
    }
}

fn is_current_epoch(epoch: u64) -> bool {
    IMAGE_CACHE_EPOCH.load(Ordering::Acquire) == epoch
}

fn notify_image_cache_invalidated() {
    if IMAGE_REPAINT_PENDING.swap(true, Ordering::AcqRel) {
        return;
    }

    let target = *IMAGE_REPAINT_HWND
        .lock()
        .expect("image repaint hwnd poisoned");
    if let Some(hwnd) = target {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(hwnd as *mut core::ffi::c_void)),
                WM_IMAGE_CACHE_INVALIDATED,
                WPARAM(0),
                LPARAM(0),
            );
        }
    } else {
        IMAGE_REPAINT_PENDING.store(false, Ordering::Release);
    }
}

fn image_dimensions(bytes: &[u8]) -> Option<(i32, i32)> {
    let image = decode_image(bytes)?;
    let dimensions = (image.width, image.height);
    drop(image);
    Some(dimensions)
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
    let box_width = (rect.right - rect.left).max(1);
    let box_height = (rect.bottom - rect.top).max(1);
    if source_width <= 0 || source_height <= 0 {
        return false;
    }

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

    DECODED_IMAGE_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if !cache.contains_key(key) {
            let Some(image) = decode_image(bytes) else {
                return false;
            };
            cache.insert(key.to_string(), image);
        }

        let Some(image) = cache.get(key) else {
            return false;
        };
        if image.width != source_width || image.height != source_height || image.image.is_null() {
            return false;
        }

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
    })
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

    #[test]
    fn remote_loader_populates_the_win32_image_cache() {
        let _gdiplus = crate::platform::win32::gdiplus::GdiPlusRuntime::start()
            .expect("start GDI+ for image cache test");
        clear_cached_image_cache();
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
        let (_, width, height) = cached_image_data(&source).expect("cached remote image");
        assert_eq!((width, height), (1, 1));
        clear_cached_image_cache();
    }
}
