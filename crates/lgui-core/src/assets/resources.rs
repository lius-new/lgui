use std::{cell::RefCell, sync::Arc};

#[cfg(feature = "svg")]
use super::SvgRenderer;
use super::{AssetResolver, CustomPaintProvider, ImageLoader};

#[derive(Clone, Default)]
pub struct RenderResources {
    resolver: Option<Arc<dyn AssetResolver>>,
    image_loader: Option<Arc<dyn ImageLoader>>,
    custom_paint: Option<Arc<dyn CustomPaintProvider>>,
    #[cfg(feature = "svg")]
    svg_renderer: Option<Arc<dyn SvgRenderer>>,
}

thread_local! {
    static CURRENT_RENDER_RESOURCES: RefCell<Vec<RenderResources>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn with_render_resources<R>(
    resources: RenderResources,
    use_resources: impl FnOnce() -> R,
) -> R {
    CURRENT_RENDER_RESOURCES.with(|current| current.borrow_mut().push(resources));
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            CURRENT_RENDER_RESOURCES.with(|current| {
                current.borrow_mut().pop();
            });
        }
    }
    let _reset = Reset;
    use_resources()
}

pub(crate) fn render_resources() -> RenderResources {
    CURRENT_RENDER_RESOURCES.with(|current| current.borrow().last().cloned().unwrap_or_default())
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
