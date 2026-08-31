mod cache;
mod custom_paint;
mod model;
mod resolver;
mod resources;

pub use cache::{clear_image_caches, request_image, ImageCacheHandle};
#[cfg(feature = "svg")]
pub use custom_paint::SvgRenderer;
pub use custom_paint::{CustomPaintProvider, SceneFragment};
pub use model::{AssetBytes, AssetError, ImageData, ImageSource, ImageStatus};
#[cfg(feature = "renderer-skia")]
pub use resolver::{http_image_loader, HttpImageLoader};
pub use resolver::{AssetResolver, ImageLoader, RemoteImageLoader, RemoteImageLoaderHandle};
pub use resources::RenderResources;

#[cfg(any(test, all(feature = "backend-winit", feature = "images")))]
pub(crate) use cache::async_image_cache;
#[cfg(feature = "renderer-skia")]
pub(crate) use cache::cached_image_bytes;
#[cfg(any(test, feature = "images"))]
pub(crate) use cache::install_image_cache;
pub(crate) use resources::{render_resources, with_render_resources};

#[cfg(test)]
mod tests;
