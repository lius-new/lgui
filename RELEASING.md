# Releasing LGUI

Every published package must declare `MIT OR Apache-2.0` in its `Cargo.toml`, and both license files
must be present before a release.

Run the test suite and inspect each package before publishing:

```powershell
cargo test -p lgui --no-default-features
cargo test -p lgui
cargo package -p lgui-skia-vulkan-windows-features --list
cargo package -p lgui-skia-vulkan-linux-features --list
cargo package -p lgui-skia-metal-features --list
cargo package -p lgui --list
```

The three feature bridge crates must exist in the registry before Cargo can verify the packaged
`lgui` crate. Publish them first and wait for the crates.io index to expose all three versions:

```powershell
cargo publish -p lgui-skia-vulkan-windows-features --dry-run
cargo publish -p lgui-skia-vulkan-windows-features
cargo publish -p lgui-skia-vulkan-linux-features --dry-run
cargo publish -p lgui-skia-vulkan-linux-features
cargo publish -p lgui-skia-metal-features --dry-run
cargo publish -p lgui-skia-metal-features
cargo publish -p lgui --dry-run
cargo publish -p lgui
```

Tag the commit that was published after all packages have been accepted:

```powershell
git tag v0.1.0
git push origin v0.1.0
```
