mod cache;
mod custom_paint;
mod model;
mod resolver;
mod resources;

pub use crate::core::{ImageCachePolicy, ImageDecodePolicy, ImageRequest};
pub use cache::{request_image, ImageCacheHandle, ImageCacheStats};
#[cfg(feature = "svg")]
pub use custom_paint::SvgRenderer;
pub use custom_paint::{CustomPaintProvider, SceneFragment};
pub use model::{AssetBytes, AssetError, ImageData, ImageSource, ImageStatus};
pub use resolver::{http_image_loader, HttpImageLoader};
pub use resolver::{
    AssetResolver, ImageLoader, RemoteImageLoader, RemoteImageLoaderHandle, RemoteImageResponse,
};
pub use resources::RenderResources;

#[cfg(any(test, all(feature = "backend-winit", feature = "images")))]
pub(crate) use cache::async_image_cache;
#[cfg(feature = "renderer-skia")]
pub(crate) use cache::cached_image_bytes;
#[cfg(any(
    test,
    feature = "images-win32",
    all(feature = "backend-winit", feature = "images")
))]
pub(crate) use cache::install_image_cache;
#[cfg(any(
    test,
    feature = "images-win32",
    all(feature = "backend-winit", feature = "images")
))]
pub(crate) use cache::update_image_reachability;
#[cfg(any(test, feature = "images-win32"))]
pub(crate) use cache::{load_url_image, prepare_image_bytes, validate_encoded_bytes};
pub(crate) use resources::{render_resources, with_render_resources};

#[cfg(test)]
mod tests;
