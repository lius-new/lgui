use std::{
    fs,
    path::{Path, PathBuf},
};

use lgui::core::ScenePrimitiveKind;

#[test]
fn source_tree_expresses_subsystem_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for directory in [
        "application",
        "assets",
        "command",
        "core",
        "diagnostics",
        "events",
        "platform",
        "renderer",
        "router",
        "runtime",
        "services",
        "store",
        "text",
        "theme",
        "window",
        "widgets",
        "core/foundation",
        "core/view",
        "core/component",
        "core/input",
        "core/layout",
        "core/scene",
        "core/component/runtime",
        "core/view/declarative",
        "core/view/tree",
        "core/scene/render",
        "router/matcher",
        "router/runtime",
        "router/declarative",
        "store/runtime",
        "runtime/host",
        "runtime/frame",
        "renderer/skia/backend",
        "platform/winit",
        "platform/winit/surface",
        "platform/win32/application",
        "platform/win32/application/host",
        "platform/win32/window",
        "platform/win32/renderer",
        "platform/win32/renderer/enhanced/gdi_renderer",
        "platform/win32/renderer/enhanced/d2d",
        "platform/win32/renderer/enhanced/static_layer",
        "platform/win32/assets",
        "platform/win32/services",
        "services/clipboard",
        "services/notification",
        "services/tray",
    ] {
        assert!(root.join(directory).is_dir(), "missing `{directory}`");
    }

    for legacy in [
        "application.rs",
        "assets.rs",
        "diagnostics.rs",
        "renderer.rs",
        "session.rs",
        "text.rs",
        "theme.rs",
        "frame",
        "host",
        "platform/skia",
        "router/table.rs",
        "router/runtime.rs",
        "core/scene/render.rs",
        "core/component/runtime.rs",
        "runtime/host/engine.rs",
        "assets/system.rs",
        "text/system.rs",
        "widgets/select/control.rs",
        "widgets/slider/control.rs",
        "services/contracts.rs",
    ] {
        assert!(
            !root.join(legacy).exists(),
            "legacy path `{legacy}` remains"
        );
    }

    for leaf in [
        "router/runtime/history.rs",
        "router/runtime/snapshot.rs",
        "router/runtime/subscription.rs",
        "router/declarative/route.rs",
        "router/declarative/builder.rs",
        "router/declarative/outlet.rs",
        "router/declarative/redirect.rs",
        "runtime/host/commit.rs",
        "runtime/host/reconcile.rs",
        "runtime/host/scene.rs",
        "runtime/host/damage.rs",
        "core/view/declarative/content.rs",
        "core/view/declarative/element.rs",
        "core/view/declarative/events.rs",
        "core/view/declarative/primitives.rs",
        "core/view/tree/events.rs",
        "core/view/tree/mutation.rs",
        "core/view/tree/scene.rs",
        "platform/win32/renderer/enhanced/static_layer/draw.rs",
        "platform/win32/renderer/enhanced/static_layer/raster.rs",
        "platform/win32/renderer/enhanced/static_layer/scroll.rs",
        "assets/resolver.rs",
        "assets/cache.rs",
        "text/layout.rs",
        "text/service.rs",
        "platform/winit/event_loop.rs",
        "platform/winit/window.rs",
        "platform/winit/renderer.rs",
        "platform/winit/input.rs",
        "window/command.rs",
        "window/manager.rs",
        "window/options.rs",
        "services/clipboard/contract.rs",
        "services/notification/contract.rs",
        "services/tray/contract.rs",
        "services/tray/model.rs",
        "platform/win32/services/notification.rs",
        "platform/win32/services/tray/host.rs",
        "platform/win32/services/tray/icon.rs",
        "platform/win32/services/tray/menu.rs",
        "platform/win32/services/tray/support.rs",
    ] {
        assert!(root.join(leaf).is_file(), "missing `{leaf}`");
    }
}

#[test]
fn source_tree_uses_real_modules_instead_of_textual_includes() {
    let violations = rust_sources("src")
        .into_iter()
        .filter_map(|(path, source)| {
            source
                .contains("include!(")
                .then(|| path.display().to_string())
        })
        .collect::<Vec<_>>();
    assert!(
        violations.is_empty(),
        "Rust source assembly must use real modules, not include!():\n{}",
        violations.join("\n")
    );

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for facade in [
        "core/view/declarative.rs",
        "core/view/tree.rs",
        "platform/win32/renderer/enhanced/static_layer.rs",
    ] {
        let source = fs::read_to_string(root.join(facade)).expect("read split facade");
        assert!(
            source.lines().count() <= 400,
            "responsibility facade `{facade}` grew beyond 400 lines"
        );
    }
}

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
    let winit_windows = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/platform/winit/windows.rs");
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
fn typed_commands_and_events_are_transport_free() {
    let forbidden = [
        "serde",
        "serde_json",
        "Payload",
        "InvokeRequest",
        "InvokeResponse",
        "crate::platform",
        "crate::store",
        "crate::router",
        "crate::frontend",
    ];
    let violations = ["src/command", "src/events"]
        .into_iter()
        .flat_map(rust_sources)
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
        "typed application capability violations:\n{}",
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
    let tray = rust_sources("src/platform/win32/services/tray")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
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

    let application = rust_sources("src/platform/win32/application")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
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
    let background = fs::read_to_string(root.join("src/platform/win32/window/background.rs"))
        .expect("read Win32 background memory lifecycle");
    for required in ["EmptyWorkingSet"] {
        assert!(
            background.contains(required),
            "background lifecycle lost `{required}`"
        );
    }

    let application = rust_sources("src/platform/win32/application")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
    for required in [
        "suspend_application_if_backgrounded",
        "session.suspend_rendering()",
        "renderer.take()",
        "render_hidden_window_once",
        "MemoryEvent::AllWindowsHidden",
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
    let gdiplus = fs::read_to_string(root.join("src/platform/win32/assets/gdiplus.rs"))
        .expect("read Win32 GDI+ lifecycle");
    for required in [
        "GdiplusStartup",
        "GdiplusShutdown",
        "trim_decoded_image_cache(0)",
    ] {
        assert!(
            gdiplus.contains(required),
            "GDI+ image lifecycle lost `{required}`"
        );
    }

    let application = rust_sources("src/platform/win32/application")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
    assert!(
        application.contains("GdiPlusRuntime::start()"),
        "Win32 application no longer starts its image runtime"
    );
}

#[test]
fn memory_governance_has_no_legacy_cache_bypasses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source = rust_sources("src")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
    for forbidden in [
        "clear_image_caches",
        "clear_render_caches",
        "release_visual_caches",
        "clear_scroll_raster_command_cache",
        "clear_svg_bitmap_cache",
        "MemoryAndDisk",
        "DEFAULT_CACHE_BUDGET",
        "DEFAULT_IMAGE_CACHE_BUDGET",
        "DEFAULT_DECODED_IMAGE_CACHE_BUDGET",
        "DEFAULT_SCROLL_RASTER_COMMAND_CACHE_BUDGET",
        "DEFAULT_SVG_CACHE_BUDGET",
        "D2D_BITMAP_CACHE_MIN_BUDGET_BYTES",
        "GDI_BITMAP_CACHE_BUDGET_BYTES",
    ] {
        assert!(
            !source.contains(forbidden),
            "legacy memory bypass `{forbidden}` remains"
        );
    }
    assert!(
        !root
            .join("src/platform/win32/renderer/enhanced/static_layer_raster_cache.rs")
            .exists(),
        "the pseudo-persistent static-layer cache returned"
    );

    let memory = rust_sources("src/memory")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
    for forbidden in [
        "windows::",
        "Win32::",
        "crate::backend",
        "crate::frontend",
        "MemoryProfile",
        "for_profile",
        "domain_weight",
        "MemoryOptions::default",
        "impl Default for MemoryOptions",
    ] {
        assert!(
            !memory.contains(forbidden),
            "portable memory governance depends on `{forbidden}`"
        );
    }

    let builder = fs::read_to_string(root.join("src/application/builder.rs"))
        .expect("read application builder");
    for required in [
        "pub struct MemoryOptionsMissing",
        "pub struct MemoryOptionsConfigured",
        "impl<B> Application<B, MemoryOptionsMissing>",
        "impl<B> Application<B, MemoryOptionsConfigured>",
    ] {
        assert!(
            builder.contains(required),
            "application construction no longer requires explicit memory policy through `{required}`"
        );
    }

    let registrations = [
        "EncodedImage",
        "DecodedImage",
        "Svg",
        "Blur",
        "StaticLayer",
        "ScrollRaster",
        "Gdi",
        "D2d",
        "Skia",
        "ComponentOutput",
        "HostScene",
        "Diagnostics",
    ];
    for domain in registrations {
        assert!(
            source.contains(&format!("CacheDomain::{domain}")),
            "cache domain `{domain}` has no registration or lifecycle adapter"
        );
    }
}

#[test]
fn portable_renderer_contract_has_no_platform_or_graphics_api_types() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let renderer = fs::read_to_string(root.join("src/renderer/contract.rs"))
        .expect("read portable renderer contract");

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
    for (module, source) in ["src/assets", "src/text"]
        .into_iter()
        .flat_map(rust_sources)
    {
        for forbidden in ["crate::platform", "windows::", "winit::", "skia_safe"] {
            assert!(
                !source.contains(forbidden),
                "portable {} contains `{forbidden}`",
                module.display()
            );
        }
    }
}

#[test]
fn portable_input_uses_complete_shared_keyboard_and_pointer_vocabulary() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let events =
        fs::read_to_string(root.join("src/core/input/event.rs")).expect("read input model");

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
    let window_options_source = fs::read_to_string(root.join("src/window/options.rs"))
        .expect("read portable window options");
    let window_options = window_options_source
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

    let win32 = rust_sources("src/platform/win32/application")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
    assert!(win32.contains("pub struct Win32WindowOptions"));
}

#[test]
fn window_and_desktop_services_have_single_owners() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let application =
        fs::read_to_string(root.join("src/application/mod.rs")).expect("read application facade");
    assert!(
        !application.contains("mod window;"),
        "application must not reclaim the top-level window subsystem"
    );

    let platform =
        fs::read_to_string(root.join("src/platform/mod.rs")).expect("read platform facade");
    assert!(
        !platform.contains("services/contracts") && !platform.contains("service_contracts"),
        "platform must consume service contracts instead of owning them through a path alias"
    );

    let violations = rust_sources("src/services")
        .into_iter()
        .flat_map(|(path, source)| {
            [
                "crate::application",
                "crate::platform",
                "windows::",
                "winit::",
            ]
            .into_iter()
            .filter_map(move |needle| {
                source
                    .contains(needle)
                    .then(|| format!("{} contains `{needle}`", path.display()))
            })
        })
        .collect::<Vec<_>>();
    assert!(
        violations.is_empty(),
        "portable desktop-service boundary violations:\n{}",
        violations.join("\n")
    );

    let tray = rust_sources("src/platform/win32/services/tray")
        .into_iter()
        .map(|(_, source)| source)
        .collect::<String>();
    for forbidden in [
        "ApplicationContext",
        "TrayRegistration",
        "main_hwnd",
        "sync_visibility",
    ] {
        assert!(
            !tray.contains(forbidden),
            "Win32 tray adapter owns application concern `{forbidden}`"
        );
    }
}

#[test]
fn desktop_service_features_separate_contracts_from_windows_adapters() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read lgui manifest");
    let feature = |name: &str| {
        manifest
            .split_once(&format!("{name} = ["))
            .unwrap_or_else(|| panic!("missing `{name}` feature"))
            .1
            .split_once(']')
            .expect("feature end")
            .0
            .to_owned()
    };

    for name in ["notifications", "tray"] {
        let body = feature(name);
        for forbidden in ["backend-win32", "windows-platform", "windows/"] {
            assert!(
                !body.contains(forbidden),
                "portable `{name}` feature depends on `{forbidden}`"
            );
        }
    }
    for (adapter, contract) in [
        ("notifications-win32", "notifications"),
        ("tray-win32", "tray"),
    ] {
        let body = feature(adapter);
        assert!(body.contains(&format!("\"{contract}\"")));
        assert!(body.contains("\"windows-platform\""));
        assert!(
            !body.contains("\"backend-win32\""),
            "`{adapter}` must not force the Win32 window backend"
        );
    }
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
    let sources = rust_sources("src/renderer/skia");
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

    let surface_adapter = root.join("src/platform/winit/surface/gl.rs");
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

#[test]
fn winit_backend_declares_its_skia_renderer_dependency() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read lgui manifest");
    let feature = manifest
        .split_once("backend-winit = [")
        .expect("backend-winit feature")
        .1
        .split_once(']')
        .expect("backend-winit feature end")
        .0;
    assert!(
        feature.contains("\"renderer-skia\""),
        "backend-winit must express its Skia software-renderer dependency"
    );
}

#[test]
fn win32_remote_images_are_owned_by_the_framework_asset_runtime() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let backend = fs::read_to_string(root.join("src/platform/win32/application/host/backend.rs"))
        .expect("read Win32 application backend");
    for required in ["install_remote_image_loader", "http_image_loader"] {
        assert!(
            backend.contains(required),
            "Win32 application lost framework remote image setup `{required}`"
        );
    }

    let cache = fs::read_to_string(root.join("src/platform/win32/assets/image_cache.rs"))
        .expect("read Win32 image cache");
    for required in ["lgui-image-loader", "crate::assets::load_url_image"] {
        assert!(
            cache.contains(required),
            "Win32 image cache lost remote loading behavior `{required}`"
        );
    }
    for forbidden in ["liugc", "crate::backend"] {
        assert!(
            !cache.contains(forbidden),
            "framework image cache leaked application dependency `{forbidden}`"
        );
    }
}
