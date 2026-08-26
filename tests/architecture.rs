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
