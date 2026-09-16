# lgui-render-api

Backend-neutral frame, damage, renderer lifecycle, capability, and memory-pressure contracts for
[LGUI](https://github.com/lius-new/lgui).

Most applications should depend on the `lgui` facade crate. Renderer and platform backend crates
use this package to share the same rendering lifecycle without introducing platform dependencies
into `lgui-core`.

Licensed under either MIT or Apache-2.0, at your option.
