use std::{
    fs,
    path::{Path, PathBuf},
};

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
        .filter(|(path, _)| !path.starts_with(&win32_backend))
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
