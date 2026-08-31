use super::*;
use crate::core::PhysicalSize;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{mpsc, Arc},
    thread,
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
