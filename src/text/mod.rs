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

pub(crate) use fonts::{
    font_assets, font_families, install_font_assets, install_font_families, FontAssets,
    FontFamilies,
};
pub(crate) use service::install_text_system;

#[cfg(test)]
mod tests;
