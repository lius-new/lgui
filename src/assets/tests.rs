use super::*;
use crate::core::PhysicalSize;
use std::{
    sync::{mpsc, Arc},
    time::Duration,
};

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
        let _guard = install_image_cache(ImageCacheHandle::new(
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
