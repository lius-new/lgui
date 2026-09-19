//! Frosted-glass window demo.
//!
//! The window is transparent and asks the Windows desktop window manager (DWM) to apply an acrylic
//! backdrop, so whatever is behind the window — the desktop, a browser, an image viewer — is
//! blurred through it. This is a *window-level* effect, distinct from `lgui`'s scene-level
//! `backdrop_blur` primitive, which blurs content drawn inside the app's own render target.
//!
//! The OS call is made here, in the example, not in the framework: a background thread finds the
//! example's own top-level window by title + process id and applies `DwmSetWindowAttribute`
//! (Windows 11) with a `SetWindowCompositionAttribute` fallback (Windows 10).

use lgui::prelude::*;
use lgui::WinitApplication;

const WINDOW_TITLE: &str = "lgui acrylic window";

fn app(cx: &mut RenderCx<'_, '_>) -> Element {
    let clicks = cx.state(0_u32);
    let value = clicks.get();

    group(UiRect::new(0.0, 0.0, 480.0, 320.0)).content((
        text(
            UiRect::new(24.0, 32.0, 456.0, 80.0),
            "Acrylic window",
            TextStyle::new(Color(0xFFFFFFFF), 28.0, 700),
        ),
        text(
            UiRect::new(24.0, 80.0, 456.0, 122.0),
            "This window blurs whatever is behind it:",
            TextStyle::new(Color(0xE0FFFFFF), 15.0, 400),
        ),
        text(
            UiRect::new(24.0, 122.0, 456.0, 164.0),
            "the desktop, a browser, or an image viewer.",
            TextStyle::new(Color(0xE0FFFFFF), 15.0, 400),
        ),
        button(
            UiRect::new(24.0, 190.0, 220.0, 242.0),
            "Click me",
            ButtonStyle {
                panel: VisualStyle::filled(Color(0x55FFFFFF)).radius(8.0),
                text: TextStyle::new(Color(0xFFFFFFFF), 15.0, 700).centered(),
                hover_outset: (2.0, 2.0),
            },
        )
        .on_click(move |_| clicks.update(|value| *value += 1)),
        text(
            UiRect::new(24.0, 250.0, 456.0, 292.0),
            format!("Clicks: {value}"),
            TextStyle::new(Color(0xE0FFFFFF), 15.0, 400),
        ),
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Apply the acrylic backdrop once the window has been created.
    #[cfg(target_os = "windows")]
    std::thread::spawn(apply_acrylic_backdrop);

    Application::with_backend(WinitApplication::new(GraphicsPreference::Auto))
        .provide(RendererKind::Skia(GraphicsPreference::Auto))
        .window_options(
            WindowOptions::new("backdrop")
                .title(WINDOW_TITLE)
                .size(Size::new(500.0, 340.0))
                .transparent(true),
        )
        .run(app)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_acrylic_backdrop() {
    use std::ffi::c_void;
    use windows::core::{s, w, PCWSTR};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWM_SYSTEMBACKDROP_TYPE, DWMSBT_TRANSIENTWINDOW,
        DWMWA_SYSTEMBACKDROP_TYPE,
    };
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};

    fn find_own_window() -> Option<HWND> {
        let title: Vec<u16> = WINDOW_TITLE
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let hwnd = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) }.ok()?;
        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        (process_id == unsafe { GetCurrentProcessId() }).then_some(hwnd)
    }

    fn dwm_acrylic(hwnd: HWND) -> bool {
        let backdrop = DWMSBT_TRANSIENTWINDOW;
        unsafe {
            DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as *const c_void,
                std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )
        }
        .is_ok()
    }

    // Windows 10 fallback: the undocumented accent-policy API.
    unsafe fn win10_acrylic(hwnd: HWND) {
        const WCA_ACCENT_POLICY: i32 = 19;
        const ACCENT_ENABLE_ACRYLICBLURBEHIND: i32 = 4;

        #[repr(C)]
        struct AccentPolicy {
            accent_state: i32,
            accent_flags: i32,
            gradient_color: u32,
            animation_id: i32,
        }
        #[repr(C)]
        struct CompositionData {
            attribute: i32,
            data: *mut c_void,
            size_of_data: usize,
        }
        type SetCompositionFn = unsafe extern "system" fn(HWND, *const CompositionData) -> i32;

        let Ok(module) = GetModuleHandleW(w!("user32.dll")) else {
            return;
        };
        let Some(procedure) = GetProcAddress(module, s!("SetWindowCompositionAttribute")) else {
            return;
        };
        let set_composition: SetCompositionFn = std::mem::transmute(procedure);

        let mut accent = AccentPolicy {
            accent_state: ACCENT_ENABLE_ACRYLICBLURBEHIND,
            accent_flags: 2,
            gradient_color: 0,
            animation_id: 0,
        };
        let data = CompositionData {
            attribute: WCA_ACCENT_POLICY,
            data: &mut accent as *mut AccentPolicy as *mut c_void,
            size_of_data: std::mem::size_of::<AccentPolicy>(),
        };
        let _ = set_composition(hwnd, &data);
    }

    for _ in 0..200 {
        if let Some(hwnd) = find_own_window() {
            if !dwm_acrylic(hwnd) {
                unsafe { win10_acrylic(hwnd) };
            }
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    eprintln!("acrylic backdrop: could not find own window");
}
