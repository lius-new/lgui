//! Platform-neutral text layout, measurement, and interaction geometry.

mod fonts;
mod layout;
mod model;
mod service;

pub use fonts::FontAsset;
pub use layout::TextLayout;
pub use model::{
    TextAffinity, TextCluster, TextDirection, TextFeature, TextFontSlant, TextFontWidth, TextHit,
    TextLayoutRequest, TextLineMetrics, TextMeasureRequest, TextMetrics, TextSpan,
    TextVerticalAlign,
};
pub use service::{layout, measure, measure_width, TextSystem, TextSystemHandle};

#[cfg(feature = "renderer-skia")]
pub(crate) use fonts::install_font_assets;
#[cfg(feature = "renderer-skia")]
pub(crate) use fonts::{font_assets, font_families, FontAssets};
pub(crate) use fonts::{install_font_families, FontFamilies};
pub(crate) use service::install_text_system;

#[path = "text_test.rs"]
#[cfg(test)]
mod tests;
