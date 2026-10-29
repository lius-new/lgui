//! Platform-neutral text layout, measurement, and interaction geometry.

mod fonts;
mod layout;
mod model;
mod rendering;
mod service;

pub use fonts::FontAsset;
pub use layout::TextLayout;
pub use model::{
    TextAffinity, TextCluster, TextDirection, TextFeature, TextFontSlant, TextFontWidth, TextHit,
    TextLayoutRequest, TextLineMetrics, TextMeasureRequest, TextMetrics, TextSpan,
    TextVerticalAlign,
};
pub use rendering::{TextHinting, TextRenderingError, TextRenderingOptions};
pub use service::{TextSystem, TextSystemHandle, layout, measure, measure_width};

pub(crate) use fonts::{FontAssets, font_assets, font_families};
pub(crate) use fonts::{FontAssetsGuard, install_font_assets};
pub(crate) use fonts::{FontFamilies, FontFamiliesGuard, install_font_families};
pub(crate) use service::{TextSystemGuard, install_text_system};

#[path = "text_test.rs"]
#[cfg(test)]
mod tests;
