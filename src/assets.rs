use std::{fmt, sync::Arc};

use crate::core::{CustomPaintStyle, Size};

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
    pub size: Size,
    pub bgra_premultiplied: AssetBytes,
}

impl ImageData {
    pub fn new(size: Size, bytes: impl Into<AssetBytes>) -> Result<Self, AssetError> {
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

pub trait CustomPaintProvider: Send + Sync + 'static {
    fn paint(
        &self,
        key: &str,
        size: Size,
        style: CustomPaintStyle,
    ) -> Result<Option<ImageData>, AssetError>;
}

#[cfg(feature = "svg")]
pub trait SvgRenderer: Send + Sync + 'static {
    fn render(&self, source: &[u8], size: Size) -> Result<ImageData, AssetError>;
}

#[derive(Clone, Default)]
pub struct RenderResources {
    resolver: Option<Arc<dyn AssetResolver>>,
    image_loader: Option<Arc<dyn ImageLoader>>,
    custom_paint: Option<Arc<dyn CustomPaintProvider>>,
    #[cfg(feature = "svg")]
    svg_renderer: Option<Arc<dyn SvgRenderer>>,
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

    #[test]
    fn image_data_rejects_invalid_stride_length() {
        let error = ImageData::new(Size::new(2, 2), Arc::<[u8]>::from([0_u8; 15]))
            .expect_err("invalid BGRA length must fail");
        assert!(matches!(error, AssetError::InvalidData(_)));
    }
}
