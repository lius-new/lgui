use std::{
    fs,
    path::{Path, PathBuf},
};

use lgui::core::ScenePrimitiveKind;

fn rust_sources(relative: &str) -> Vec<(PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    let mut pending = vec![root];
    let mut sources = Vec::new();
    while let Some(path) = pending.pop() {
        if path.is_dir() {
            for entry in fs::read_dir(&path).expect("read architecture test directory") {
                pending.push(entry.expect("read architecture test entry").path());
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            let source = fs::read_to_string(&path).expect("read Rust source for architecture test");
            sources.push((path, source));
        }
    }
    sources
}

#[test]
fn runtime_has_no_application_platform_or_backend_dependencies() {
    let win32_backend = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/platform/win32");
    let winit_windows = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/platform/winit_windows.rs");
    let forbidden = [
        "crate::frontend",
        "native::windows",
        "frontend::pages",
        "frontend::state",
        "frontend::runtime",
        "frontend::components",
        "frontend::theme",
        "use windows::",
        "windows::Win32",
        "Win32::",
        "Graphics::Gdi",
        "Direct2D",
    ];
    let violations = rust_sources("src")
        .into_iter()
        .filter(|(path, _)| !path.starts_with(&win32_backend) && path != &winit_windows)
        .flat_map(|(path, source)| {
            forbidden.iter().filter_map(move |needle| {
                source
                    .contains(needle)
                    .then(|| format!("{} contains `{needle}`", path.display()))
            })
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "GUI architecture boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn win32_backend_has_no_application_dependencies() {
    let forbidden = [
        "crate::frontend",
        "native::windows",
        "frontend::pages",
        "frontend::state",
        "frontend::runtime",
        "frontend::components",
        "frontend::theme",
        "AppRuntime",
        "Liuguang",
        "LIUGC_",
    ];
    let violations = rust_sources("src/platform/win32")
        .into_iter()
        .flat_map(|(path, source)| {
            forbidden.iter().filter_map(move |needle| {
                source
                    .contains(needle)
                    .then(|| format!("{} contains `{needle}`", path.display()))
            })
        })
        .collect::<Vec<_>>();

    assert!(
        violations.is_empty(),
        "Win32 backend boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn tray_host_owns_a_message_loop_outside_the_application_window_thread() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let tray =
        fs::read_to_string(root.join("src/platform/win32/tray.rs")).expect("read Win32 tray host");
    for required in [
        ".name(\"lgui-tray\".to_string())",
        "run_tray_thread(",
        "GetMessageW(&mut message",
        "TrackPopupMenu(",
    ] {
        assert!(
            tray.contains(required),
            "independent tray host lost `{required}`"
        );
    }

    let application = fs::read_to_string(root.join("src/platform/win32/application.rs"))
        .expect("read Win32 application host");
    for forbidden in ["handle_tray_message", "show_tray_menu", "TRAY_MESSAGE_ID"] {
        assert!(
            !application.contains(forbidden),
            "application UI thread reclaimed tray responsibility through `{forbidden}`"
        );
    }
}

#[test]
fn background_memory_optimization_is_owned_by_the_win32_window_lifecycle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let background = fs::read_to_string(root.join("src/platform/win32/background.rs"))
        .expect("read Win32 background memory lifecycle");
    for required in [
        "release_visual_caches",
        "clear_cached_image_cache",
        "clear_blur_caches",
        "EmptyWorkingSet",
    ] {
        assert!(
            background.contains(required),
            "background lifecycle lost `{required}`"
        );
    }

    let application = fs::read_to_string(root.join("src/platform/win32/application.rs"))
        .expect("read Win32 application host");
    for required in [
        "suspend_application_if_backgrounded",
        "session.suspend_rendering()",
        "renderer.take()",
        "render_hidden_window_once",
    ] {
        assert!(
            application.contains(required),
            "window lifecycle lost `{required}`"
        );
    }
}

#[test]
fn image_runtime_is_owned_by_the_win32_application_lifecycle() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let gdiplus = fs::read_to_string(root.join("src/platform/win32/gdiplus.rs"))
        .expect("read Win32 GDI+ lifecycle");
    for required in [
        "GdiplusStartup",
        "GdiplusShutdown",
        "clear_decoded_image_cache",
    ] {
        assert!(
            gdiplus.contains(required),
            "GDI+ image lifecycle lost `{required}`"
        );
    }

    let application = fs::read_to_string(root.join("src/platform/win32/application.rs"))
        .expect("read Win32 application host");
    assert!(
        application.contains("GdiPlusRuntime::start()"),
        "Win32 application no longer starts its image runtime"
    );
}

#[test]
fn portable_renderer_contract_has_no_platform_or_graphics_api_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let renderer =
        fs::read_to_string(root.join("src/renderer.rs")).expect("read portable renderer contract");

    for required in [
        "pub trait SceneRenderer",
        "pub struct FrameInfo",
        "pub struct RendererCapabilities",
        "pub enum MemoryPressure",
    ] {
        assert!(
            renderer.contains(required),
            "portable renderer contract lost `{required}`"
        );
    }
    for forbidden in [
        "trait RenderBackend",
        "windows::",
        "Win32",
        "winit::",
        "skia_safe",
        "freya_skia_safe",
        "crate::platform",
    ] {
        assert!(
            !renderer.contains(forbidden),
            "portable renderer contract contains `{forbidden}`"
        );
    }

    let sources = rust_sources("src");
    assert!(
        sources
            .iter()
            .all(|(_, source)| !source.contains("pub trait Win32Renderer:")),
        "legacy Win32 renderer lifecycle still exists"
    );
}

#[test]
fn portable_asset_and_text_services_have_no_platform_dependencies() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for module in ["assets.rs", "icons.rs", "text.rs"] {
        let source = fs::read_to_string(root.join("src").join(module))
            .unwrap_or_else(|error| panic!("read portable {module}: {error}"));
        for forbidden in ["crate::platform", "windows::", "winit::", "skia_safe"] {
            assert!(
                !source.contains(forbidden),
                "portable {module} contains `{forbidden}`"
            );
        }
    }
}

#[test]
fn portable_input_uses_complete_shared_keyboard_and_pointer_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let events = fs::read_to_string(root.join("src/core/event.rs")).expect("read input model");

    for required in [
        "pub use keyboard_types",
        "KeyboardEvent",
        "PointerData",
        "PointerId",
        "PointerKind",
        "WheelDelta",
        "TouchPhase",
        "ImeEvent",
        "PlatformEvent",
    ] {
        assert!(events.contains(required), "input model lost `{required}`");
    }
    for forbidden in [
        "enum KeyCode",
        "InputEvent::Backspace",
        "InputEvent::KeyDown",
        "InputEvent::ImeStart",
        "InputEvent::ImeUpdate",
    ] {
        assert!(
            !events.contains(forbidden),
            "input model restored legacy `{forbidden}`"
        );
    }
}

#[test]
fn portable_window_options_do_not_own_win32_policy() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let application =
        fs::read_to_string(root.join("src/application.rs")).expect("read application model");
    let window_options = application
        .split_once("pub struct WindowOptions")
        .expect("WindowOptions declaration")
        .1
        .split_once("impl WindowOptions")
        .expect("WindowOptions implementation")
        .0;
    for forbidden in ["class_name", "icon_bytes", "rounded_corners"] {
        assert!(
            !window_options.contains(forbidden),
            "portable WindowOptions owns `{forbidden}`"
        );
    }

    let win32 = fs::read_to_string(root.join("src/platform/win32/application.rs"))
        .expect("read Win32 window options");
    assert!(win32.contains("pub struct Win32WindowOptions"));
}

#[test]
fn skia_design_assigns_every_scene_primitive() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let design = fs::read_to_string(root.join("SKIA_DESIGN.md")).expect("read Skia design");

    for kind in ScenePrimitiveKind::ALL {
        assert!(
            design.contains(kind.as_str()),
            "Skia design does not assign Scene primitive `{}`",
            kind.as_str()
        );
    }

    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("read lgui manifest");
    assert!(!manifest.contains("../freya"));
    assert!(!manifest.contains("freya-skia-safe"));
}

#[test]
fn portable_skia_contains_no_window_system_adapter_code() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let sources = rust_sources("src/platform/skia");
    let forbidden = [
        "windows::",
        "Win32::",
        "HWND",
        "HDC",
        "HGLRC",
        "WGL",
        "StretchDIBits",
        "SwapBuffers",
        "raw_window_handle",
    ];
    let violations = sources
        .into_iter()
        .flat_map(|(path, source)| {
            forbidden.iter().filter_map(move |needle| {
                source
                    .contains(needle)
                    .then(|| format!("{} contains `{needle}`", path.display()))
            })
        })
        .collect::<Vec<_>>();
    assert!(
        violations.is_empty(),
        "portable Skia boundary violations:\n{}",
        violations.join("\n")
    );

    let surface_adapter = root.join("src/platform/winit_skia_gl.rs");
    assert!(
        surface_adapter.is_file(),
        "winit OpenGL Skia surface adapter is missing"
    );
    assert!(
        !root.join("src/platform/win32/skia.rs").exists(),
        "Skia window surfaces must not have a second Win32 renderer path"
    );
    let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("read lgui manifest");
    assert!(!manifest.contains("renderer-skia-wgl"));
}

#[test]
fn framework_renderers_contain_no_liuguang_business_paint_keys() {
    let forbidden = [
        "flow_rail",
        "game_platform.nav",
        "hazard.stripes",
        "Liuguang",
        "LIUGC_",
    ];
    let violations = rust_sources("src/platform")
        .into_iter()
        .flat_map(|(path, source)| {
            forbidden.iter().filter_map(move |needle| {
                source
                    .contains(needle)
                    .then(|| format!("{} contains `{needle}`", path.display()))
            })
        })
        .collect::<Vec<_>>();
    assert!(
        violations.is_empty(),
        "framework renderer contains application behavior:\n{}",
        violations.join("\n")
    );
}

#[test]
fn portable_skia_feature_does_not_enable_win32_backend() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read lgui manifest");
    let feature = manifest
        .split_once("renderer-skia = [")
        .expect("renderer-skia feature")
        .1
        .split_once(']')
        .expect("renderer-skia feature end")
        .0;
    assert!(!feature.contains("backend-win32"));
    assert!(!feature.contains("windows/"));
}
