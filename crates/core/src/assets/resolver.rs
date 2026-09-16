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

    fn load_response(
        &self,
        url: &str,
        _etag: Option<&str>,
        _last_modified: Option<&str>,
    ) -> Result<RemoteImageResponse, AssetError> {
        self.load(url).map(RemoteImageResponse::from_bytes)
    }
}

#[derive(Clone, Debug)]
pub struct RemoteImageResponse {
    pub bytes: Option<AssetBytes>,
    pub mime: Option<String>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub not_modified: bool,
}

impl RemoteImageResponse {
    pub fn from_bytes(bytes: AssetBytes) -> Self {
        Self {
            bytes: Some(bytes),
            mime: None,
            etag: None,
            last_modified: None,
            not_modified: false,
        }
    }

    fn not_modified() -> Self {
        Self {
            bytes: None,
            mime: None,
            etag: None,
            last_modified: None,
            not_modified: true,
        }
    }
}

#[derive(Clone)]
#[allow(dead_code)]
pub struct RemoteImageLoaderHandle(Arc<dyn RemoteImageLoader>);

impl RemoteImageLoaderHandle {
    pub fn new(loader: impl RemoteImageLoader) -> Self {
        Self(Arc::new(loader))
    }

    #[doc(hidden)]
    pub fn load(&self, url: &str) -> Result<AssetBytes, AssetError> {
        self.0.load(url)
    }

    #[cfg(feature = "persistent-cache")]
    pub(crate) fn load_response(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<RemoteImageResponse, AssetError> {
        self.0.load_response(url, etag, last_modified)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HttpImageLoader;

impl RemoteImageLoader for HttpImageLoader {
    fn load(&self, url: &str) -> Result<AssetBytes, AssetError> {
        self.load_response(url, None, None)?
            .bytes
            .ok_or_else(|| AssetError::InvalidData("image response had no body".to_owned()))
    }

    fn load_response(
        &self,
        url: &str,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<RemoteImageResponse, AssetError> {
        const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;
        if !url.starts_with("https://") && !url.starts_with("http://") {
            return Err(AssetError::Unsupported(
                "remote images must use http or https".to_owned(),
            ));
        }
        let mut request = ureq::get(url);
        if let Some(etag) = etag {
            request = request.set("If-None-Match", etag);
        }
        if let Some(last_modified) = last_modified {
            request = request.set("If-Modified-Since", last_modified);
        }
        let response = match request.call() {
            Ok(response) if response.status() == 304 => {
                return Ok(RemoteImageResponse::not_modified())
            }
            Ok(response) => response,
            Err(ureq::Error::Status(304, _)) => return Ok(RemoteImageResponse::not_modified()),
            Err(error) => return Err(AssetError::NotFound(error.to_string())),
        };
        let mime = response.header("Content-Type").map(str::to_owned);
        let response_etag = response.header("ETag").map(str::to_owned);
        let response_last_modified = response.header("Last-Modified").map(str::to_owned);
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
        Ok(RemoteImageResponse {
            bytes: Some(Arc::from(bytes)),
            mime,
            etag: response_etag,
            last_modified: response_last_modified,
            not_modified: false,
        })
    }
}

pub fn http_image_loader() -> RemoteImageLoaderHandle {
    RemoteImageLoaderHandle::new(HttpImageLoader)
}
