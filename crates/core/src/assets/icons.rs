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
    icons: HashMap<&'static str, SvgIconSource>,
}

impl SvgIconRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_icon(mut self, key: &'static str, svg: impl Into<SvgIconSource>) -> Self {
        self.icons.insert(key, svg.into());
        self
    }

    pub fn with_icon_bytes(mut self, key: &'static str, bytes: &'static [u8]) -> Self {
        self.icons
            .insert(key, SvgIconSource::Bytes(Cow::Borrowed(bytes)));
        self
    }

    pub fn with_icon_text(mut self, key: &'static str, svg: &'static str) -> Self {
        self.icons
            .insert(key, SvgIconSource::Text(Cow::Borrowed(svg)));
        self
    }

    pub(crate) fn resolve(&self, key: &str) -> Option<Cow<'static, str>> {
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
pub(crate) struct IconRegistration(pub Arc<SvgIconRegistry>);

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

#[cfg(feature = "renderer-skia")]
pub(crate) fn resolve_svg(key: &str) -> Option<Cow<'static, str>> {
    CURRENT_ICON_REGISTRY.with(|current| {
        current
            .borrow()
            .last()
            .and_then(|registry| registry.resolve(key))
            .or_else(|| builtin_svg(key))
    })
}

pub(crate) fn builtin_svg(key: &str) -> Option<Cow<'static, str>> {
    match key {
        "copy" => Some(Cow::Borrowed(COPY)),
        "arrow-left" => Some(Cow::Borrowed(ARROW_LEFT)),
        "close" => Some(Cow::Owned(icondata_to_svg(icondata::LuX))),
        "minus" => Some(Cow::Owned(icondata_to_svg(icondata::LuMinus))),
        _ => None,
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
