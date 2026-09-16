use std::{
    ffi::c_void,
    sync::{Mutex, OnceLock},
};

use windows::{
    core::{Interface, Result, HSTRING, PCWSTR},
    Win32::{
        Foundation::{HANDLE, SIZE},
        Graphics::{
            DirectWrite::{
                DWriteCreateFactory, IDWriteFactory, IDWriteFactory2, IDWriteFontFallback,
                IDWriteTextFormat, IDWriteTextFormat1, DWRITE_FACTORY_TYPE_SHARED,
                DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT,
                DWRITE_HIT_TEST_METRICS, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
                DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_UNICODE_RANGE, DWRITE_WORD_WRAPPING_NO_WRAP,
            },
            Gdi::{
                AddFontMemResourceEx, CreateCompatibleDC, CreateFontW, DeleteDC, DeleteObject,
                GetGlyphIndicesW, GetTextExtentPoint32W, RemoveFontMemResourceEx, SelectObject,
                SetTextCharacterExtra, CLEARTYPE_QUALITY, DEFAULT_CHARSET, DEFAULT_PITCH,
                FF_DONTCARE, GGI_MARK_NONEXISTING_GLYPHS, HDC, HFONT, OUT_TT_ONLY_PRECIS,
            },
        },
    },
};

use lgui_core::{
    core::UiRect,
    text::{TextMeasureRequest, TextMetrics, TextSystem, TextSystemHandle},
};

const DEFAULT_UI_FONT_FAMILIES: &[&str] = &[
    "Microsoft YaHei UI",
    "Segoe UI",
    "Segoe UI Emoji",
    "Segoe UI Symbol",
];

struct UiFontFamilies {
    names: Vec<&'static str>,
    wide: Vec<Vec<u16>>,
}

impl UiFontFamilies {
    fn new(families: &'static [&'static str]) -> Self {
        let names = if families.is_empty() {
            DEFAULT_UI_FONT_FAMILIES.to_vec()
        } else {
            families.to_vec()
        };
        let wide = names
            .iter()
            .map(|family| {
                let mut wide: Vec<u16> = family.encode_utf16().collect();
                wide.push(0);
                wide
            })
            .collect();
        Self { names, wide }
    }
}

static UI_FONT_FAMILIES: OnceLock<UiFontFamilies> = OnceLock::new();
static DWRITE_FONT_FALLBACK: OnceLock<Option<IDWriteFontFallback>> = OnceLock::new();
static PRIVATE_FONT_HANDLES: Mutex<Vec<usize>> = Mutex::new(Vec::new());

pub fn set_ui_font_family(family: &'static str) {
    let mut wide: Vec<u16> = family.encode_utf16().collect();
    wide.push(0);
    let _ = UI_FONT_FAMILIES.set(UiFontFamilies {
        names: vec![family],
        wide: vec![wide],
    });
}

pub fn set_ui_font_families(families: &'static [&'static str]) {
    let _ = UI_FONT_FAMILIES.set(UiFontFamilies::new(families));
}

fn ui_font_families() -> &'static UiFontFamilies {
    UI_FONT_FAMILIES.get_or_init(|| UiFontFamilies::new(DEFAULT_UI_FONT_FAMILIES))
}

pub fn ui_font_family() -> PCWSTR {
    PCWSTR(ui_font_families().wide[0].as_ptr())
}

pub fn ui_font_family_at(index: usize) -> PCWSTR {
    ui_font_families()
        .wide
        .get(index)
        .map(|family| PCWSTR(family.as_ptr()))
        .unwrap_or_else(ui_font_family)
}

pub fn ui_font_family_count() -> usize {
    ui_font_families().wide.len()
}

pub fn ui_font_family_names() -> &'static [&'static str] {
    &ui_font_families().names
}

pub fn apply_dwrite_font_fallback(
    factory: &IDWriteFactory,
    format: &IDWriteTextFormat,
) -> Result<()> {
    let Some(fallback) = DWRITE_FONT_FALLBACK
        .get_or_init(|| create_dwrite_font_fallback(factory).ok())
        .as_ref()
    else {
        return Ok(());
    };

    unsafe {
        let format1: IDWriteTextFormat1 = format.cast()?;
        format1.SetFontFallback(fallback)?;
    }
    Ok(())
}

fn create_dwrite_font_fallback(factory: &IDWriteFactory) -> Result<IDWriteFontFallback> {
    let families = ui_font_families();
    if families.wide.len() <= 1 {
        unsafe {
            let factory2: IDWriteFactory2 = factory.cast()?;
            return factory2.GetSystemFontFallback();
        }
    }

    unsafe {
        let factory2: IDWriteFactory2 = factory.cast()?;
        let builder = factory2.CreateFontFallbackBuilder()?;
        let ranges = [DWRITE_UNICODE_RANGE {
            first: 0x0000,
            last: 0x10FFFF,
        }];
        let target_families: Vec<*const u16> = families
            .wide
            .iter()
            .skip(1)
            .map(|family| family.as_ptr())
            .collect();
        let locale = HSTRING::from("zh-cn");
        builder.AddMapping(
            &ranges,
            &target_families,
            None,
            &locale,
            ui_font_family(),
            1.0,
        )?;
        if let Ok(system_fallback) = factory2.GetSystemFontFallback() {
            let _ = builder.AddMappings(&system_fallback);
        }
        builder.CreateFontFallback()
    }
}

pub fn measure_gdi_text_width_with_fallback(
    text: &str,
    height: i32,
    weight: i32,
    tracking: i32,
) -> Option<i32> {
    if text.is_empty() {
        return Some(0);
    }
    unsafe {
        let hdc = CreateCompatibleDC(None);
        if hdc.is_invalid() {
            return None;
        }
        let width = measure_gdi_text_runs_width(hdc, text, height, weight, tracking);
        let _ = DeleteDC(hdc);
        width
    }
}

/// Measures a single unwrapped line using DirectWrite with a GDI fallback.
///
/// `font_height` follows the GDI signed-height convention. DirectWrite uses its absolute value.
/// Controls remain independent from either native text API.
pub fn measure_text_width(
    text: &str,
    bounds: UiRect,
    font_height: f32,
    font_weight: i32,
) -> Option<f32> {
    measure_dwrite_text_width(text, bounds, font_height.abs().ceil() as u32, font_weight)
        .map(|width| width as f32)
        .or_else(|| {
            measure_gdi_text_width_with_fallback(text, font_height.round() as i32, font_weight, 0)
                .map(|width| width as f32)
        })
}

pub(crate) struct Win32TextSystem;

impl TextSystem for Win32TextSystem {
    fn measure(&self, request: &TextMeasureRequest<'_>) -> Option<TextMetrics> {
        measure_text_width(
            request.text,
            request.bounds,
            request.font_height,
            request.font_weight,
        )
        .map(|width| TextMetrics { width })
    }
}

pub fn portable_text_system_handle() -> TextSystemHandle {
    TextSystemHandle::new(Win32TextSystem)
}

fn measure_dwrite_text_width(
    text: &str,
    bounds: UiRect,
    font_size: u32,
    font_weight: i32,
) -> Option<i32> {
    if text.is_empty() {
        return Some(0);
    }
    unsafe {
        let factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED).ok()?;
        let locale = HSTRING::from("zh-cn");
        let format = factory
            .CreateTextFormat(
                ui_font_family(),
                None,
                DWRITE_FONT_WEIGHT(font_weight),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font_size.max(1) as f32,
                &locale,
            )
            .ok()?;
        format
            .SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING)
            .ok()?;
        format
            .SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR)
            .ok()?;
        format.SetWordWrapping(DWRITE_WORD_WRAPPING_NO_WRAP).ok()?;
        apply_dwrite_font_fallback(&factory, &format).ok()?;
        let wide: Vec<u16> = text.encode_utf16().collect();
        let layout = factory
            .CreateTextLayout(
                &wide,
                &format,
                bounds.width().max(1.0),
                bounds.height().max(1.0),
            )
            .ok()?;
        let mut point_x = 0.0;
        let mut point_y = 0.0;
        let mut metrics = DWRITE_HIT_TEST_METRICS::default();
        layout
            .HitTestTextPosition(
                wide.len().saturating_sub(1) as u32,
                true,
                &mut point_x,
                &mut point_y,
                &mut metrics,
            )
            .ok()?;
        Some(point_x.round() as i32)
    }
}

unsafe fn measure_gdi_text_runs_width(
    hdc: HDC,
    text: &str,
    height: i32,
    weight: i32,
    tracking: i32,
) -> Option<i32> {
    let mut total_width = 0;
    let mut run_family_index: Option<usize> = None;
    let mut run = String::new();

    for ch in text.chars() {
        let family_index = gdi_font_family_for_char(hdc, ch, height, weight);
        if run_family_index == Some(family_index) {
            run.push(ch);
            continue;
        }
        if !run.is_empty() {
            total_width += measure_gdi_text_run_width(
                hdc,
                &run,
                height,
                weight,
                tracking,
                run_family_index.unwrap_or(0),
            )?;
        }
        run.clear();
        run.push(ch);
        run_family_index = Some(family_index);
    }

    if !run.is_empty() {
        total_width += measure_gdi_text_run_width(
            hdc,
            &run,
            height,
            weight,
            tracking,
            run_family_index.unwrap_or(0),
        )?;
    }
    Some(total_width)
}

unsafe fn gdi_font_family_for_char(hdc: HDC, ch: char, height: i32, weight: i32) -> usize {
    (0..ui_font_family_count())
        .find(|family_index| gdi_font_family_supports_char(hdc, ch, height, weight, *family_index))
        .unwrap_or(0)
}

unsafe fn gdi_font_family_supports_char(
    hdc: HDC,
    ch: char,
    height: i32,
    weight: i32,
    family_index: usize,
) -> bool {
    let Some(font) = create_gdi_font(height, weight, family_index) else {
        return false;
    };
    let old_font = SelectObject(hdc, font.into());
    let mut utf16 = [0u16; 2];
    let units = ch.encode_utf16(&mut utf16);
    let mut glyphs = vec![0u16; units.len()];
    let result = GetGlyphIndicesW(
        hdc,
        PCWSTR(units.as_ptr()),
        units.len() as i32,
        glyphs.as_mut_ptr(),
        GGI_MARK_NONEXISTING_GLYPHS,
    );
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font.into());
    result != u32::MAX && glyphs.iter().all(|glyph| *glyph != 0xFFFF)
}

unsafe fn measure_gdi_text_run_width(
    hdc: HDC,
    text: &str,
    height: i32,
    weight: i32,
    tracking: i32,
    family_index: usize,
) -> Option<i32> {
    let font = create_gdi_font(height, weight, family_index)?;
    let old_font = SelectObject(hdc, font.into());
    let previous_extra = SetTextCharacterExtra(hdc, tracking);
    let wide: Vec<u16> = text.encode_utf16().collect();
    let mut size = SIZE::default();
    let measured = GetTextExtentPoint32W(hdc, &wide, &mut size).as_bool();
    let _ = SetTextCharacterExtra(hdc, previous_extra);
    let _ = SelectObject(hdc, old_font);
    let _ = DeleteObject(font.into());
    measured.then_some(size.cx)
}

unsafe fn create_gdi_font(height: i32, weight: i32, family_index: usize) -> Option<HFONT> {
    let font = CreateFontW(
        height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET,
        OUT_TT_ONLY_PRECIS,
        windows::Win32::Graphics::Gdi::CLIP_DEFAULT_PRECIS,
        CLEARTYPE_QUALITY,
        (DEFAULT_PITCH.0 | FF_DONTCARE.0) as u32,
        ui_font_family_at(family_index),
    );
    (!font.is_invalid()).then_some(font)
}

pub fn install_private_font(bytes: &'static [u8]) {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    INSTALLED.get_or_init(|| unsafe {
        let mut font_count = 0;
        let handle = AddFontMemResourceEx(
            bytes.as_ptr().cast::<c_void>(),
            bytes.len() as u32,
            None,
            &mut font_count,
        );
        if !handle.is_invalid() {
            PRIVATE_FONT_HANDLES
                .lock()
                .expect("private font handles poisoned")
                .push(handle.0 as usize);
        }
    });
}

pub fn release_private_fonts() {
    let mut handles = PRIVATE_FONT_HANDLES
        .lock()
        .expect("private font handles poisoned");
    for handle in handles.drain(..) {
        unsafe {
            let _ = RemoveFontMemResourceEx(HANDLE(handle as *mut c_void));
        }
    }
}
