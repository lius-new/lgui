# LGUI Core

`lgui-core` contains LGUI's platform-neutral application runtime, retained component tree, layout,
input, scene, window, resource, and service contracts. Its `runtime` module owns session, host, and
frame orchestration; concrete backend selection remains in the `lgui` facade.

Applications should normally depend on [`lgui`](https://crates.io/crates/lgui) instead of using
this package directly.

## Relative placement in the public tree

`UiElement::relative_to(RelativePlacement { target, relation, order })` attaches
an element relative to an existing node's `public_id`. `Before` and `After`
insert it among the target's siblings; `Append` adds it to a container's children.
Projection resolves the logical reference before layout, preserving component
identity, event callbacks, and retained input state when the destination changes.
Elements sharing an attachment are sorted by `order` and stable public identity.

Missing or collapsed targets, invalid container destinations, and cycles return
the element to its original projection parent. Applications can use a collapsed
staging parent to hide unresolved elements until their targets appear. Public IDs
are resolved again on each projection, so recreating a target does not invalidate
the logical reference. This mechanism is independent of application-specific
mount points or extension identifiers.
