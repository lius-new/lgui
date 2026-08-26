// Custom paint is an immediate rasterization boundary.
//
// Callers may pass animation-frame state through `CustomPaintStyle` (for example hover
// intensity). Treat the returned pixels as per-frame output. Backend bitmap caches must
// not cache these results by default.
use crate::{
    assets::render_resources,
    core::{CustomPaintStyle, Size},
};

pub fn custom_paint_bgra(
    key: &str,
    width: i32,
    height: i32,
    style: CustomPaintStyle,
) -> Option<Vec<u8>> {
    render_resources()
        .custom_paint()?
        .paint(key, Size::new(width, height), style)
        .ok()??
        .bgra_premultiplied
        .as_ref()
        .to_vec()
        .into()
}
