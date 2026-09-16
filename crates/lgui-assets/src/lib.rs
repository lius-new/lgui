#![deny(unsafe_code)]

#[cfg(feature = "images")]
#[doc(hidden)]
pub mod backend;
#[cfg(feature = "images")]
mod cache;
#[cfg(feature = "images")]
mod custom_paint;
#[cfg(feature = "svg")]
mod extension;
#[cfg(feature = "svg")]
pub mod icons;
#[cfg(feature = "images")]
mod model;
#[cfg(feature = "images")]
mod resolver;
#[cfg(feature = "images")]
mod resources;

#[cfg(feature = "images")]
use lgui_core::{core, memory};

#[cfg(any(test, feature = "images-win32"))]
#[doc(hidden)]
pub use cache::{load_url_image, prepare_image_bytes, validate_encoded_bytes};
#[cfg(feature = "images")]
pub use cache::{request_image, ImageCacheHandle, ImageCacheStats};
#[cfg(feature = "svg")]
pub use custom_paint::SvgRenderer;
#[cfg(feature = "images")]
pub use custom_paint::{CustomPaintProvider, SceneFragment};
#[cfg(feature = "svg")]
pub use extension::AssetsApplicationExt;
#[cfg(feature = "images")]
pub use lgui_core::core::{ImageCachePolicy, ImageDecodePolicy, ImageRequest};
#[cfg(feature = "images")]
pub use model::{AssetBytes, AssetError, ImageData, ImageSource, ImageStatus};
#[cfg(feature = "images")]
pub use resolver::{http_image_loader, HttpImageLoader};
#[cfg(feature = "images")]
pub use resolver::{
    AssetResolver, ImageLoader, RemoteImageLoader, RemoteImageLoaderHandle, RemoteImageResponse,
};
#[cfg(feature = "images")]
pub use resources::RenderResources;

#[path = "assets_test.rs"]
#[cfg(all(test, feature = "images"))]
mod tests;
