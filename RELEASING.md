# Releasing LGUI

Every published package must declare `MIT OR Apache-2.0` in its `Cargo.toml`, and both license files
must be present before a release.

Run the test suite and inspect each package before publishing:

```powershell
cargo fmt --all -- --check
cargo test --workspace --all-features
cargo package -p lgui-core
cargo package -p lgui-render-api --list
cargo package -p lgui-router --list
cargo package -p lgui-store --list
cargo package -p lgui-widgets --list
cargo package -p lgui-render-skia --list
cargo package -p lgui-platform-win32 --list
cargo package -p lgui-render-win32 --list
cargo package -p lgui-platform-winit --list
cargo package -p lgui --list
```

Workspace packages must be published in dependency order. After each step, wait for crates.io to
index that version before running the next package's dry run:

```powershell
cargo publish -p lgui-core --dry-run
cargo publish -p lgui-core
cargo publish -p lgui-render-api --dry-run
cargo publish -p lgui-render-api
cargo publish -p lgui-router --dry-run
cargo publish -p lgui-router
cargo publish -p lgui-store --dry-run
cargo publish -p lgui-store
cargo publish -p lgui-widgets --dry-run
cargo publish -p lgui-widgets
cargo publish -p lgui-render-skia --dry-run
cargo publish -p lgui-render-skia
cargo publish -p lgui-platform-win32 --dry-run
cargo publish -p lgui-platform-win32
cargo publish -p lgui-render-win32 --dry-run
cargo publish -p lgui-render-win32
cargo publish -p lgui-platform-winit --dry-run
cargo publish -p lgui-platform-winit
cargo publish -p lgui --dry-run
cargo publish -p lgui
```

`lgui-skia-vulkan-windows-features`, `lgui-skia-vulkan-linux-features`, and
`lgui-skia-metal-features` were 0.1 compatibility packages. They are no longer workspace members,
but their 0.1 releases remain available so existing `lgui 0.1` dependency resolution keeps working.

Tag the commit that was published after all packages have been accepted:

```powershell
git tag v0.2.0
git push origin v0.2.0
```
