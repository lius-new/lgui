use std::io::Read;
use std::sync::Arc;

use super::{AssetBytes, AssetError, ImageData};

pub trait AssetResolver: Send + Sync + 'static {
    fn resolve(&self, id: &str) -> Result<AssetBytes, AssetError>;
}

pub trait ImageLoader: Send + Sync + 'static {
    fn decode(&self, bytes: &[u8]) -> Result<ImageData, AssetError>;
}

pub trait RemoteImageLoader: Send + Sync + 'static {
    fn load(&self, url: &str) -> Result<AssetBytes, AssetError>;
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct RemoteImageLoaderHandle(Arc<dyn RemoteImageLoader>);

impl RemoteImageLoaderHandle {
    pub fn new(loader: impl RemoteImageLoader) -> Self {
        Self(Arc::new(loader))
    }

    pub(crate) fn load(&self, url: &str) -> Result<AssetBytes, AssetError> {
        self.0.load(url)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HttpImageLoader;

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

pub fn http_image_loader() -> RemoteImageLoaderHandle {
    RemoteImageLoaderHandle::new(HttpImageLoader)
}
