use std::{cell::RefCell, sync::Arc};

#[derive(Clone, Copy)]
pub(crate) struct FontFamilies(pub &'static [&'static str]);

thread_local! {
    static FONT_FAMILIES: RefCell<Vec<&'static [&'static str]>> = const { RefCell::new(Vec::new()) };
}

pub(crate) struct FontFamiliesGuard;

impl Drop for FontFamiliesGuard {
    fn drop(&mut self) {
        FONT_FAMILIES.with(|current| {
            current.borrow_mut().pop();
        });
    }
}

pub(crate) fn install_font_families(families: &'static [&'static str]) -> FontFamiliesGuard {
    FONT_FAMILIES.with(|current| current.borrow_mut().push(families));
    FontFamiliesGuard
}

#[cfg(feature = "renderer-skia")]
pub(crate) fn font_families() -> &'static [&'static str] {
    FONT_FAMILIES.with(|current| current.borrow().last().copied().unwrap_or(&["Segoe UI"]))
}

#[derive(Clone, Debug)]
pub struct FontAsset {
    pub bytes: Arc<Vec<u8>>,
    pub family_alias: Option<String>,
}

impl FontAsset {
    pub fn new(bytes: impl Into<Arc<Vec<u8>>>) -> Self {
        Self {
            bytes: bytes.into(),
            family_alias: None,
        }
    }

    pub fn family_alias(mut self, alias: impl Into<String>) -> Self {
        self.family_alias = Some(alias.into());
        self
    }
}

#[cfg(feature = "renderer-skia")]
#[derive(Clone, Default)]
pub(crate) struct FontAssets(pub Arc<Vec<FontAsset>>);

#[cfg(feature = "renderer-skia")]
thread_local! {
    static FONT_ASSETS: RefCell<Vec<Arc<Vec<FontAsset>>>> = const { RefCell::new(Vec::new()) };
}

#[cfg(feature = "renderer-skia")]
pub(crate) struct FontAssetsGuard;

#[cfg(feature = "renderer-skia")]
impl Drop for FontAssetsGuard {
    fn drop(&mut self) {
        FONT_ASSETS.with(|current| {
            current.borrow_mut().pop();
        });
    }
}

#[cfg(feature = "renderer-skia")]
pub(crate) fn install_font_assets(assets: Arc<Vec<FontAsset>>) -> FontAssetsGuard {
    FONT_ASSETS.with(|current| current.borrow_mut().push(assets));
    FontAssetsGuard
}

#[cfg(feature = "renderer-skia")]
pub(crate) fn font_assets() -> Arc<Vec<FontAsset>> {
    FONT_ASSETS.with(|current| current.borrow().last().cloned().unwrap_or_default())
}
