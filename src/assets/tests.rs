use super::*;
use crate::core::PhysicalSize;
use std::{
    io::Cursor,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Arc,
    },
    thread,
    time::Duration,
};

#[cfg(feature = "persistent-cache")]
use std::sync::Mutex;

const ONE_PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xB5, 0x1C, 0x0C,
    0x02, 0x00, 0x00, 0x00, 0x0B, 0x49, 0x44, 0x41, 0x54, 0x78, 0xDA, 0x63, 0x64, 0xF8, 0x0F, 0x00,
    0x01, 0x05, 0x01, 0x01, 0x27, 0x18, 0xE3, 0x66, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44,
    0xAE, 0x42, 0x60, 0x82,
];

struct TestRemoteLoader;

impl RemoteImageLoader for TestRemoteLoader {
    fn load(&self, _url: &str) -> Result<AssetBytes, AssetError> {
        Ok(Arc::from(ONE_PIXEL_PNG))
    }
}

struct FailingRemoteLoader {
    calls: Arc<AtomicUsize>,
}

impl RemoteImageLoader for FailingRemoteLoader {
    fn load(&self, _url: &str) -> Result<AssetBytes, AssetError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(AssetError::NotFound("fixture failure".to_owned()))
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
    let request =
        crate::core::ImageRequest::new(ImageSource::url("https://example.invalid/image.png"));
    assert_eq!(request_image(&request), ImageStatus::Failed);
    {
        let _guard = install_image_cache(ImageCacheHandle::new(|_| ImageStatus::Loading, |_| None));
        assert_eq!(request_image(&request), ImageStatus::Loading);
    }
    assert_eq!(request_image(&request), ImageStatus::Failed);
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
        crate::memory::MemoryGovernor::default(),
    );
    let request =
        crate::core::ImageRequest::new(ImageSource::url("https://example.invalid/test.png"));
    assert_eq!(cache.request(&request), ImageStatus::Loading);
    woke.recv_timeout(Duration::from_secs(2))
        .expect("image completion wake");
    assert_eq!(cache.request(&request), ImageStatus::Ready);
    assert_eq!(cache.bytes(&request).as_deref(), Some(ONE_PIXEL_PNG));
}

#[test]
fn async_image_cache_coalesces_in_flight_failure_and_applies_retry_backoff() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (wake, woke) = mpsc::channel();
    let cache = async_image_cache(
        RemoteImageLoaderHandle::new(FailingRemoteLoader {
            calls: Arc::clone(&calls),
        }),
        move || {
            let _ = wake.send(());
        },
        1024,
        crate::memory::MemoryGovernor::default(),
    );
    let request =
        crate::core::ImageRequest::new(ImageSource::url("https://example.invalid/fail.png"));

    assert_eq!(cache.request(&request), ImageStatus::Loading);
    assert!(matches!(
        cache.request(&request),
        ImageStatus::Loading | ImageStatus::Failed
    ));
    woke.recv_timeout(Duration::from_secs(2))
        .expect("image failure wake");
    assert_eq!(cache.request(&request), ImageStatus::Failed);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn reachability_is_aggregated_across_window_owners() {
    let (wake, woke) = mpsc::channel();
    let cache = async_image_cache(
        RemoteImageLoaderHandle::new(TestRemoteLoader),
        move || {
            let _ = wake.send(());
        },
        1024,
        crate::memory::MemoryGovernor::default(),
    );
    let first =
        crate::core::ImageRequest::new(ImageSource::url("https://example.invalid/first.png"))
            .cache_policy(crate::core::ImageCachePolicy::WhileVisible);
    let second =
        crate::core::ImageRequest::new(ImageSource::url("https://example.invalid/second.png"))
            .cache_policy(crate::core::ImageCachePolicy::WhileVisible);
    let first_owner = crate::memory::DomainInstanceId(1);
    let second_owner = crate::memory::DomainInstanceId(2);

    cache.update_reachability(first_owner, std::slice::from_ref(&first));
    cache.update_reachability(second_owner, std::slice::from_ref(&second));
    assert_eq!(cache.request(&first), ImageStatus::Loading);
    assert_eq!(cache.request(&second), ImageStatus::Loading);
    for _ in 0..2 {
        woke.recv_timeout(Duration::from_secs(2))
            .expect("image completion wake");
    }
    assert_eq!(cache.stats().entries, 2);

    cache.update_reachability(first_owner, &[]);
    assert_eq!(cache.stats().entries, 1);
    assert_eq!(cache.request(&second), ImageStatus::Ready);

    cache.update_reachability(second_owner, &[]);
    assert_eq!(cache.stats().entries, 0);
}

#[test]
fn target_decode_downsamples_large_sources() {
    let source = image::DynamicImage::new_rgba8(64, 32);
    let mut encoded = Cursor::new(Vec::new());
    source
        .write_to(&mut encoded, image::ImageFormat::Png)
        .expect("encode image fixture");
    let request =
        crate::core::ImageRequest::new(ImageSource::url("https://example.invalid/large.png"))
            .decode_policy(crate::core::ImageDecodePolicy::FitTarget(
                PhysicalSize::new(16, 16),
            ));

    let resized = prepare_image_bytes(
        Arc::from(encoded.into_inner()),
        &request,
        crate::memory::MemoryOptions::balanced().budget,
    )
    .expect("downsample image");
    let decoded = image::load_from_memory(resized.as_ref()).expect("decode resized image");

    assert!(decoded.width() <= 16);
    assert!(decoded.height() <= 16);
    assert_eq!((decoded.width(), decoded.height()), (16, 8));
}

#[cfg(feature = "persistent-cache")]
#[test]
fn persistent_image_revalidation_reuses_cached_bytes_after_304() {
    struct NotModifiedLoader {
        etag: Arc<Mutex<Option<String>>>,
    }

    impl RemoteImageLoader for NotModifiedLoader {
        fn load(&self, _url: &str) -> Result<AssetBytes, AssetError> {
            Err(AssetError::NotFound("load_response expected".to_owned()))
        }

        fn load_response(
            &self,
            _url: &str,
            etag: Option<&str>,
            _last_modified: Option<&str>,
        ) -> Result<RemoteImageResponse, AssetError> {
            *self.etag.lock().expect("etag capture poisoned") = etag.map(ToOwned::to_owned);
            Ok(RemoteImageResponse {
                bytes: None,
                mime: None,
                etag: None,
                last_modified: None,
                not_modified: true,
            })
        }
    }

    let root = std::env::temp_dir().join(format!(
        "lgui-image-304-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let store = crate::memory::FileCacheStore::new(&root);
    let url = "https://example.invalid/avatar.png";
    let key = crate::memory::PersistentCacheKey::new("avatars", url, 3);
    let mut entry = crate::memory::PersistentEntry::new(key.clone(), b"cached-avatar".to_vec());
    entry.etag = Some("fixture-etag".to_owned());
    entry.mime = Some("image/png".to_owned());
    entry.expires_unix_seconds = Some(1);
    crate::memory::PersistentCacheStore::put(&store, entry).expect("seed persistent image");
    let governor = crate::memory::MemoryGovernor::with_store(
        crate::memory::MemoryOptions::balanced(),
        Some(Arc::new(store.clone())),
    );
    let observed_etag = Arc::new(Mutex::new(None));
    let loader = RemoteImageLoaderHandle::new(NotModifiedLoader {
        etag: Arc::clone(&observed_etag),
    });
    let request = crate::core::ImageRequest::new(ImageSource::url(url))
        .namespace("avatars")
        .version(3)
        .cache_policy(crate::core::ImageCachePolicy::Persistent {
            max_age: Duration::from_secs(60),
            revalidate: true,
        });

    let bytes = load_url_image(&loader, &governor, &request, url)
        .expect("304 should reuse persistent bytes");

    assert_eq!(bytes.as_ref(), b"cached-avatar");
    assert_eq!(
        observed_etag
            .lock()
            .expect("etag capture poisoned")
            .as_deref(),
        Some("fixture-etag")
    );
    assert!(crate::memory::PersistentCacheStore::get(&store, &key)
        .expect("read refreshed entry")
        .is_some());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn http_image_loader_fetches_remote_bytes() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind image fixture server");
    let address = listener.local_addr().expect("fixture server address");
    let server = thread::spawn(move || {
        let (mut connection, _) = listener.accept().expect("accept image request");
        let mut request = [0_u8; 1024];
        let _ = connection.read(&mut request).expect("read image request");
        connection
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: close\r\n\r\nLGUi")
            .expect("write image response");
    });

    let bytes = HttpImageLoader
        .load(&format!("http://{address}/avatar.png"))
        .expect("fetch image fixture");

    assert_eq!(bytes.as_ref(), b"LGUi");
    server.join().expect("image fixture server");
}
