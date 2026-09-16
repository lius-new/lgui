use lgui_core::application::Application;

use crate::icons::{IconRegistration, SvgIconRegistry};

pub trait AssetsApplicationExt: Sized {
    fn svg_icons(self, registry: SvgIconRegistry) -> Self;
}

impl<B, M> AssetsApplicationExt for Application<B, M> {
    fn svg_icons(self, registry: SvgIconRegistry) -> Self {
        self.provide(IconRegistration(std::sync::Arc::new(registry)))
    }
}
