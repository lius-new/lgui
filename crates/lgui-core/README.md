# LGUI Core

`lgui-core` contains LGUI's platform-neutral application runtime, retained component tree, layout,
input, scene, window, resource, and service contracts. Its `runtime` module owns session, host, and
frame orchestration; concrete backend selection remains in the `lgui` facade.

Applications should normally depend on [`lgui`](https://crates.io/crates/lgui) instead of using
this package directly.
