//! Portable SVG icon data supplied to an Application.

use std::{borrow::Cow, cell::RefCell, collections::HashMap, sync::Arc};

#[derive(Clone, Debug)]
pub enum SvgIconSource {
    Text(Cow<'static, str>),
    Bytes(Cow<'static, [u8]>),
}

impl SvgIconSource {
    pub(crate) fn to_svg(&self) -> Option<Cow<'static, str>> {
        match self {
            Self::Text(svg) => Some(svg.clone()),
            Self::Bytes(bytes) => std::str::from_utf8(bytes)
                .ok()
                .map(|svg| Cow::Owned(svg.to_owned())),
        }
    }
}

#[derive(Clone, Default)]
pub struct SvgIconRegistry {
    icons: HashMap<Arc<str>, SvgIconSource>,
}

impl SvgIconRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_icon(mut self, key: impl Into<Arc<str>>, svg: impl Into<SvgIconSource>) -> Self {
        self.icons.insert(key.into(), svg.into());
        self
    }

    pub fn with_icon_bytes(mut self, key: impl Into<Arc<str>>, bytes: &'static [u8]) -> Self {
        self.icons
            .insert(key.into(), SvgIconSource::Bytes(Cow::Borrowed(bytes)));
        self
    }

    pub fn with_icon_text(mut self, key: impl Into<Arc<str>>, svg: &'static str) -> Self {
        self.icons
            .insert(key.into(), SvgIconSource::Text(Cow::Borrowed(svg)));
        self
    }

    #[doc(hidden)]
    pub fn resolve(&self, key: &str) -> Option<Cow<'static, str>> {
        self.icons
            .get(key)
            .and_then(SvgIconSource::to_svg)
            .or_else(|| builtin_svg(key))
    }
}

impl From<&'static str> for SvgIconSource {
    fn from(value: &'static str) -> Self {
        Self::Text(Cow::Borrowed(value))
    }
}

impl From<&'static [u8]> for SvgIconSource {
    fn from(value: &'static [u8]) -> Self {
        Self::Bytes(Cow::Borrowed(value))
    }
}

impl From<icondata::Icon> for SvgIconSource {
    fn from(value: icondata::Icon) -> Self {
        Self::Text(Cow::Owned(icondata_to_svg(value)))
    }
}

#[derive(Clone)]
#[doc(hidden)]
pub struct IconRegistration(pub Arc<SvgIconRegistry>);

thread_local! {
    static CURRENT_ICON_REGISTRY: RefCell<Vec<Arc<SvgIconRegistry>>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn with_icon_registry<R>(
    registry: Option<Arc<SvgIconRegistry>>,
    use_registry: impl FnOnce() -> R,
) -> R {
    if let Some(registry) = registry {
        CURRENT_ICON_REGISTRY.with(|current| current.borrow_mut().push(registry));
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                CURRENT_ICON_REGISTRY.with(|current| {
                    current.borrow_mut().pop();
                });
            }
        }
        let _reset = Reset;
        use_registry()
    } else {
        use_registry()
    }
}

#[cfg(feature = "svg")]
pub(crate) fn resolve_svg(key: &str) -> Option<Cow<'static, str>> {
    CURRENT_ICON_REGISTRY.with(|current| {
        current
            .borrow()
            .last()
            .and_then(|registry| registry.resolve(key))
            .or_else(|| builtin_svg(key))
    })
}

#[doc(hidden)]
pub fn builtin_svg(key: &str) -> Option<Cow<'static, str>> {
    match key {
        "copy" => Some(Cow::Borrowed(COPY)),
        "arrow-left" => Some(Cow::Borrowed(ARROW_LEFT)),
        "close" => Some(Cow::Owned(icondata_to_svg(icondata::LuX))),
        "minus" => Some(Cow::Owned(icondata_to_svg(icondata::LuMinus))),
        _ => inline_path_svg(key),
    }
}

/// A bounded, origin-zero filled path supplied by the caller, rather than a
/// registry alias. Reuse the ordinary SVG icon cache, color and DPI handling.
fn inline_path_svg(key: &str) -> Option<Cow<'static, str>> {
    if key.len() > 256 {
        return None;
    }
    let (view, data) = key.strip_prefix("svg-path:")?.split_once(':')?;
    let (width, height) = view.split_once('x')?;
    let width: f32 = width.parse().ok()?;
    let height: f32 = height.parse().ok()?;
    if ![width, height]
        .into_iter()
        .all(|v| v.is_finite() && (1.0..=4096.0).contains(&v))
        || data.is_empty()
        || !data.bytes().all(|b| {
            b.is_ascii_digit()
                || b.is_ascii_whitespace()
                || b"MmZzLlHhVvCcSsQqTtAaEe.,+-".contains(&b)
        })
    {
        return None;
    }
    // The whitelist excludes XML delimiters and attribute quotes. No resource
    // references, scripts, fonts or other SVG elements can be introduced.
    Some(Cow::Owned(format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {width} {height}\"><path fill=\"currentColor\" d=\"{data}\"/></svg>"
    )))
}

#[cfg(test)]
mod inline_path_tests {
    use super::*;
    #[test]
    fn inline_paths_use_the_normal_icon_resolver_without_registration() {
        let key = "svg-path:20x20:M10 2A8 8 0 0 0 2 10H4A6 6 0 0 1 10 4Z";
        let source = SvgIconRegistry::new().resolve(key).unwrap();
        assert!(source.contains("viewBox=\"0 0 20 20\""));
        assert!(source.contains("fill=\"currentColor\""));
        assert!(source.contains("M10 2A8"));
        assert!(builtin_svg(key).is_some());
        assert!(SvgIconRegistry::new().resolve("close").is_some());
    }
    #[test]
    fn inline_paths_reject_xml_resources_and_unbounded_input() {
        for key in [
            "svg-path:0x24:M0 0",
            "svg-path:NaNx24:M0 0",
            "svg-path:4097x24:M0 0",
            "svg-path:24x24:",
            "svg-path:24x24:<svg/>",
            "svg-path:24x24:M0 0\"/>",
            "svg-path:24x24:url(test)",
        ] {
            assert!(inline_path_svg(key).is_none(), "{key}");
        }
        assert!(inline_path_svg(&format!("svg-path:24x24:M0 0{}", "L1 1".repeat(100))).is_none());
    }
}

fn icondata_to_svg(icon: icondata::Icon) -> String {
    let mut svg = String::from("<svg xmlns=\"http://www.w3.org/2000/svg\"");
    push_svg_attr(&mut svg, "style", icon.style);
    push_svg_attr(&mut svg, "x", icon.x);
    push_svg_attr(&mut svg, "y", icon.y);
    push_svg_attr(&mut svg, "width", icon.width);
    push_svg_attr(&mut svg, "height", icon.height);
    push_svg_attr(&mut svg, "viewBox", icon.view_box);
    push_svg_attr(&mut svg, "stroke-linecap", icon.stroke_linecap);
    push_svg_attr(&mut svg, "stroke-linejoin", icon.stroke_linejoin);
    push_svg_attr(&mut svg, "stroke-width", icon.stroke_width);
    push_svg_attr(&mut svg, "stroke", icon.stroke);
    push_svg_attr(&mut svg, "fill", icon.fill);
    svg.push_str(" opacity=\"currentOpacity\">");
    svg.push_str(icon.data);
    svg.push_str("</svg>");
    svg
}

fn push_svg_attr(svg: &mut String, name: &str, value: Option<&'static str>) {
    if let Some(value) = value {
        svg.push(' ');
        svg.push_str(name);
        svg.push_str("=\"");
        svg.push_str(value);
        svg.push('"');
    }
}

const COPY: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" opacity="currentOpacity"><rect x="9" y="9" width="11" height="11" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>"#;
const ARROW_LEFT: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" opacity="currentOpacity"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>"#;
