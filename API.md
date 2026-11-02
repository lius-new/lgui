# LGUI API

## Application And Windows

`Application::with_backend(backend).memory_options(...).window_options(...).run(root)` creates the
main window and mounts the root component. Use `WinitApplication::new(GraphicsPreference)` for the
desktop stack.
`.provide(value)` adds Application-scoped typed data. A `RendererKind` resource selects the renderer
used by a compatible backend. Optional `.tray(...)` and `.notifications(...)` configure the built-in Windows
adapters when `tray-win32` and `notifications-win32` are enabled. `.notification_service(...)`
installs a portable application-provided adapter, and `.executor(...)` configures task execution.

Components access `cx.application()`, `cx.windows()`, and event-context `cx.window()` handles.
Auxiliary windows are declared with `WindowOptions`; `.owner(id)` declares an explicit owner.
Business code never receives native handles.

`WindowOptions::text_rendering(options)` supplies per-window
`lgui::text::TextRenderingOptions`. Applications may explicitly override LCD
coverage gamma/contrast; defaults retain backend values. Update one window with
`WindowManager::set_text_rendering(id, options)` or
`WindowHandle::set_text_rendering(options)`, which requests a full repaint when
values change. These parameters currently apply to software LCD rendering.
Theme selection and the choice of coverage values remain application policy.

## Child Sizing

`Dimension` and `SizeConstraints` describe child-owned sizes independently of the
parent's arrangement. Set them with `.size_constraints(...)` on `Element`,
`UiElement`, or `UiNode`. Unspecified constraints preserve declared geometry.

```rust,ignore
use lgui::prelude::{Dimension, Size, SizeConstraints};

let sizing = SizeConstraints::new(Dimension::Percent(0.5), Dimension::Auto)
    .min_size(Size::new(80.0, 20.0))
    .max_size(Size::new(240.0, 120.0))
    .aspect_ratio(2.0);
let item = group(rect).size_constraints(sizing);
```

`Points` uses logical units; `Percent(0.5)` uses half the parent's content size,
after layout padding. `Auto` preserves the declared size. `Content` measures text
through the installed text system or measures a container's children. A definite
text width constrains word wrapping when measuring content height. Without a text
system, declared geometry remains the fallback; plain content strings now use
measured widths when a text system is available.

An omitted maximum is unbounded. Minimums win over contradictory maximums, and
explicit dimensions and size bounds take precedence over the preferred aspect
ratio. Invalid lengths normalize to zero; invalid aspect ratios are ignored.
Content changes invalidate containing layouts; paint-only text color/alpha changes
reuse measurements. Parent resizing also updates retained percentage-sized children.
Sizing does not select an overflow policy. Set `.overflow(Overflow::Clip)` for a
clipped subtree, `Ellipsis` for measured, grapheme-safe text truncation, or
`Collapse(Size::new(min_available_width, min_available_height))` to remove a node
from layout when its parent's available size is below the threshold. Ellipsis
preserves the full semantic text. `Visibility::Hidden` preserves layout space;
`Collapsed` removes it. Hidden subtrees neither paint nor accept focus/pointers.

## Layout And Position

`flex(rect, axis)` replaces `stack`. Its `.gap`, `.padding`, `.align`, `.justify`
and `.wrap(true)` configure `LayoutSpec::Flex(FlexLayout)`. Child `.flex_item`
accepts `FlexItem { grow, shrink, basis, align_self }`. Shrink is weighted by the
basis; allocation freezes children at their minimum/maximum before distributing
remaining space. Wrapping supplies the Wrap capability; there is no second Wrap
layout. `Justify` includes Start, Center, End, SpaceBetween, SpaceAround and
SpaceEvenly; `Align` includes Start, Center, End and Stretch.

`group(rect).layout(LayoutSpec::Dock { padding, gap })` arranges children with
`.dock(DockSide::Top/Bottom/Left/Right)`, followed by a Fill child. Insertion order
determines which edges consume space first. `LayoutSpec::Grid(GridLayout)` accepts
column/row `GridTrack::Points`, `Content` and `Fraction`;
`.grid_cell(GridCell::at(column, row).span(columns, rows))` selects/spans tracks. Unspecified cells are
placed in unoccupied slots. `GridLayout::gap` supplies independent row/column gaps.
All these layouts consume the same `SizeConstraints` and text measurement service.

Position is a separate node property:

```rust,ignore
let floating = group(rect)
    .position(Position::anchor("toolbar.button", AnchorSide::Bottom))
    .z_index(10);
let badge = group(rect).position(Position::Layer {
    horizontal: Align::End, vertical: Align::Start,
    offset: lgui::core::Point::new(-4.0, 4.0),
});
```

`Position::Absolute(Point)` offsets from the parent's content origin; Layer aligns
and offsets against that content rectangle. Both are outside sibling allocation.
Sibling `z_index` is stable for equal values and controls paint and hit order.
Anchor refers to a public ID, supports four sides, offset/gap, optional flipping,
then clamps its origin to the viewport. Constrain oversized floating content with
`SizeConstraints`; origin clamping cannot make oversized content fit. Missing or
cyclic anchors are returned by `LayoutRuntime::position_errors`.
`Position::Sticky { top, left }` preserves its flow slot and sticks to the nearest
scroll clip; use Auto to leave an axis unstuck. Coordinates are logical pixels.

Migration: remove `Stack`/`stack` calls in favor of Flex; replace
`LayoutSpec::Absolute` with `LayoutSpec::None` plus `Position::Absolute` where an
offset is required. Replace `LayoutSpec::Fixed(size)` with Points constraints.
`LayoutSpec` is Clone rather than Copy because Grid owns its tracks.

## Portable Controls

`text_input(rect, State<TextInputModel>)` and `multiline_input` use the scoped text
system for selection, caret geometry, pointer hit testing and scrolling. Their
model supports grapheme editing, range selection, undo/redo and IME preedit without
committing partial composition. Builders provide placeholder, read_only, password,
style, clipboard callbacks, on_change and on_selection. Password display and
semantic values are masked; copy/cut are disabled. `TextClipboard` is an injected
pair of callbacks and does not introduce a platform dependency.

```rust,ignore
let editor = text_input(bounds, cx.state(TextInputModel::new("Hello")))
    .public_id("editor")
    .on_identified_event(|event| {
        // Committed Change and Selection include event.node, the public string ID.
    });
```

`progress_ring(rect, Option<f32>, color)` is determinate for Some and uses existing
frame scheduling for None. `scrollbar(rect, axis, viewport_extent, content_extent,
offset_state)` supports track clicks and thumb dragging. `resize_handle(rect,
axis, value, min, max, callback)` clamps drag values. `split(rect, axis, ratio_state,
(first_min, second_min), first, second)` uses Flex and writes back a normalized
ratio; if the viewport is smaller than both minimums, it divides the available
extent proportionally. Callbacks receive the two pane rectangles.

`virtual_list(rect, Arc<RowHeights>, offset_state, render_row)` only constructs the
visible range plus one overscan row per side. `RowHeights::uniform` has constant
storage; `variable` stores measured heights and prefix sums for binary range
lookup. Recreate variable heights when measurements change. An explicit row key
is preserved; otherwise the row index supplies identity. The list supports wheel,
PageUp/PageDown/Home/End and an integrated scrollbar.

`tree_view(rect, nodes, expanded_state, selected_state, offset_state)` uses stable
TreeNode IDs and virtualizes expanded rows. Arrows/Home/End navigate and expand;
collapsed branches are omitted. `tabs(rect, labels, selected_state, render_page)`
renders the active page and supports arrow/Home/End switching. Tab selection is
exposed in semantics. `badge(rect, label, optional_color)` and
`icon_button(rect, owned_key, accessible_label)` supply the smaller controls;
attach ordinary click handlers to the icon button.

## Public Nodes, Runtime Descriptions And Behaviors

`.public_id(owned_string)` is optional and unique within one HostTree/window.
Empty or duplicate IDs are rejected; a duplicate never replaces an existing node.
`HostTree::public_node` and `public_roots` return public IDs, nearest named parents,
named children, bounds and readable PublicSemantics. Private intermediary nodes
and internal UiIds are absent from this projection. Semantic access requires no
accessibility feature. Native accessibility remains a separate platform adapter.

`RuntimeOutput::identified_events(tree)` maps click, raw input, pointer, key, IME,
focus and registered handler events to the nearest public ancestor.
`identified_payload(name, payload)` maps application-defined selection/change
events, and `visibility_events(previous_tree)` reports show/hide/removal. Input
controls also offer the unified committed-event callback shown above.

`DescriptionTree::new(namespace, mount_public_id).update(&mut tree, descriptions)`
accepts runtime-built `NodeDescription` trees. Properties include layouts, sizing,
position, visibility, dynamic text/icons, semantics and interaction. Keys are unique
within one DescriptionTree and retain identity across reordering; unchanged nodes retain shared
storage. Only changed properties/structure are applied, removed nodes are reclaimed,
and unowned mount children are preserved. The complete description is validated
before mutation, so duplicate key/ID and mount/collision errors are atomic.
`DescriptionUpdate` reports created/updated/reused/removed counts and public
visibility events. `clear` removes that description tree. Commit the updated tree
through the existing UiSession; no second UI runtime is created.

`.on_outside_pointer`, `.focus_scope()` and `.on_close_requested` provide popup
behavior primitives. Outside events target each registered node once, without
ancestor rebubbling. Escape issues a close request to the active scope/handler;
it does not delete UI. Existing focus traversal traps Tab in the scope and restores
previous focus after scope removal. The application owns popup existence and stacking.

Icon and custom paint keys now own `Arc<str>` data throughout nodes, scene commands
and SvgIconRegistry. Builders accept String, Arc or literals; direct enum/scene
construction uses `.into()`. `UiNodeKind` is Clone rather than Copy. Runtime SVG
data uses `SvgIconSource::Text(Cow::Owned(svg))`; no leaked strings are needed.

ThemeTokens contains colors, spacing, typography, radii and motion. New controls
read ThemeContext defaults; explicit styles and colors remain supported.
`VisibilityTransition::from_theme(tokens)` reads motion/spacing defaults.
`presence(rect, visible, transition, child)` composes opacity/translation on the
existing compositing layer and component frame runtime. Exit disables interaction
immediately, retains the child while animating, then collapses it; reversing the
target continues from current progress. Zero duration changes immediately.

Run the independent gallery with `cargo run --example capabilities --features
renderer-skia-software`. See IMPLEMENTATION.md for requirement-level evidence and
the separate, deferred Loom integration gate.

## Memory And Resource Lifetime

`lgui` has no default memory profile or framework-owned byte budgets. Every application must construct
and inject a complete `MemoryOptions` before `run`; the builder typestate intentionally provides
`run` only after `.memory_options(...)`. The application owns the policy name, global soft/hard
limits, every domain budget, lifecycle actions, default image retention, persistent-cache choice,
and any user-facing profiles. `MemoryOptions::unbounded(default_image_policy,
persistent_cache_enabled)` is an explicit application opt-out; even its persistent-cache choice
must be supplied by the application, and it is never an implicit fallback.

```rust,ignore
let memory = MemoryOptions::new(
    "my-app-low-memory",
    MemoryBudget::new(
        32 * MIB, // managed cache soft limit
        48 * MIB, // managed cache hard limit
        16 * MIB, // transient hard limit
        128 * MIB as u64,
        8 * MIB,  // maximum encoded resource
        16 * MIB, // maximum decoded resource
        1,        // parallel large tasks
    ),
    application_domain_budgets(),
    application_event_policy(),
    ImageCachePolicy::WhileVisible,
    true,
);

Application::with_backend(backend)
    .memory_options(memory)
    .persistent_cache(lgui::memory::FileCacheStore::new(cache_root.join("lgui")))
    .run(app)?;
```

`MemoryDomainBudgets` assigns an exact total to each domain. If multiple live instances register
the same domain, the Governor divides only that domain's total between them and rebalances when
instances enter or leave. A zero domain budget is preserved as zero and can disable storage in
that cache. `lgui` does not derive domain weights, minimum cache sizes, profile ratios, or hard
limits. Applications must keep simultaneously active domain totals coherent with their global
budget; alternative renderer domains do not need to be summed when only one can be active.
Framework-owned caches start disabled until the application Governor assigns their domain budget,
and backends forward that value without finite caps or nonzero minimums. A reachable image may
temporarily appear as pinned bytes while it is delivered even when its reusable-cache budget is
zero; after it becomes unreachable, no reusable entry is retained.

`MemoryEventPolicy` maps each lifecycle event to `MemoryAction::None`, `EnforceBudget`, or an
explicit scoped Trim target. Backends report events, but the application decides their effects.
During lifecycle notification the Governor enforces a configured global budget only when the
selected action requests it; `set_options()` also rebalances and enforces the new policy immediately.
`MemoryOptions::validate()` rejects an empty policy name, soft limits above hard limits, zero
large-task concurrency, single-resource limits above the transient hard limit, nonzero encoded or
decoded image domain budgets below their corresponding single-resource limits, and an unresolved
`ApplicationDefault` image policy. A zero image-domain budget remains the explicit way to disable
that reusable cache.

At runtime, `ApplicationContext::memory()` returns the application Governor. Use `snapshot()` for
domain-level diagnostics, `set_options()` only when application-owned policy changes at runtime,
`notify()` for a real lifecycle or pressure event, and `trim()` for an explicit scoped request. Do not call
cache-specific clear functions. Win32 adapters post and coalesce native Trim work onto their owning
UI thread, so refresh the snapshot after the UI event has run.
`CacheScope::Memory` and `AllRebuildable` target registered in-memory domains; `Persistent`
targets only the application-injected persistent store, including when normal persistent reads are
disabled. Scope selection never crosses that boundary implicitly.

Framework integrations that own a cache register a `DomainRegistration` through
`ApplicationContext::register_memory_domain`. The adapter must report bytes and stats and, for a
managed domain, accept its assigned budget. A native adapter's Trim callback must dispatch to its
owner thread rather than moving or dropping native objects in the Governor caller.

`ImageRequest::new` uses `ImageCachePolicy::ApplicationDefault`; each application resolves it from
its injected policy. A request can override that default explicitly:

```rust,ignore
let request = ImageRequest::new(lgui::core::UiImageSource::url(avatar_url))
    .cache_policy(ImageCachePolicy::Persistent {
        max_age: std::time::Duration::from_secs(7 * 24 * 60 * 60),
        revalidate: true,
    })
    .decode_policy(ImageDecodePolicy::FitTarget(lgui::core::PhysicalSize::new(128, 128)))
    .priority(CachePriority::High)
    .namespace("avatars")
    .version(avatar_revision);

Element::requested_image(id, rect, request, lgui::core::ImageFit::Cover)
```

`NoStore` retains the completed request only for its element lifecycle; it does not redownload on
every frame. `WhileVisible` becomes an early eviction candidate when no committed Scene in any
window references the key. `Scene` and `Session` retain longer but remain budget-bound.
`Persistent` writes only non-sensitive compressed responses when a store is enabled. Mark
authenticated or private content with `.sensitive(true)`.

The built-in remote-image loader validates HTTPS against the operating system certificate store.
`lgui` does not read application private-CA configuration or provide a certificate-validation
bypass; development roots must be installed into the operating system trust store.

Static raster reuse is explicit and has no disk-like mode:

```rust,ignore
StaticLayerSpec::new(StaticLayerSource::runtime())
    .cache_policy(RasterCachePolicy::memory(
        RetentionClass::Scene,
        CachePriority::Normal,
    ))
    .revision("profile-card-v2")
```

Component State and Effects are correctness state and are never ordinary cache entries. Memory
pressure may discard committed Component output and Host/Scene projections; the next render
rebuilds them while preserving State and Effect ownership.

## Commands And Events

Applications define typed command contracts and register their service-backed handlers on the
Application builder. The command name is diagnostic metadata; dispatch uses the Rust command type
and does not serialize arguments or results.

```rust,ignore
struct Login;

impl Command for Login {
    type Args = LoginRequest;
    type Output = AuthSession;
    type Error = AuthError;

    const NAME: &'static str = "auth.login";
}

Application::with_backend(backend)
    .memory_options(application_memory_options())
    .command::<Login>(move |_cx, request| {
        let auth = auth.clone();
        async move { auth.login(request).await }
    })
    .executor(executor)
    .run(app)?;
```

Async UI handlers run inside their owning Application scope, so they can call the receiver-free
`invoke` and `emit` functions after an await or after moving between executor threads. The
configured Application executor runs the handler.

```rust,ignore
button(rect, "Login", style).on_click_async(move |_ui| {
    let request = request.clone();
    let set_session = set_session.clone();
    async move {
        set_session(invoke::<Login>(request).await.ok());
    }
})
```

Controlled widgets with value callbacks use the same adapter without a Store requirement:

```rust,ignore
checkbox(rect, checked, async_handler_with(move |_ui, checked| async move {
    invoke::<SetPreference>(checked).await.ok();
}))
```

Async Effects inherit the same Application scope:

```rust,ignore
cx.use_async_effect(user_id.clone(), move || async move {
    set_profile(invoke::<LoadProfile>(user_id).await.ok());
});
```

Events are keyed, typed Application broadcasts. `EventKey<T>` makes the key the real routing
identity while preserving compile-time payload types. `emit(key, payload).await` returns the number
of current listeners after synchronous listeners return and asynchronous listener tasks are
scheduled; it does not await those tasks. `listen` and `listen_async` are Effect-owned and
unsubscribe when their key changes or their component unmounts.

```rust,ignore
#[derive(Clone)]
struct DownloadProgress { received: u64, total: u64 }

const DOWNLOAD_PROGRESS: EventKey<DownloadProgress> =
    EventKey::new("download.progress");

listen(DOWNLOAD_PROGRESS, move |progress| {
    set_progress(progress.received as f32 / progress.total as f32);
});

emit(DOWNLOAD_PROGRESS, DownloadProgress { received, total }).await;
```

Use `EventKey::dynamic(name)` for validated runtime keys. Emitting an unobserved dynamic key does
not create a retained topic. A key name cannot be bound to conflicting payload types while it has
active subscriptions. External executors do not implicitly inherit an Application; wrap their
Future once with `application.scope(future)`.

Use Commands for typed request/response work, Events for ephemeral broadcasts, State for local
component values, and Store for shared renderable snapshots. None requires another.

## State And Effects

`cx.state(value)` returns a generation-aware `State<T>`. `set` and `update` enqueue only the owning
component. `cx.use_effect(dependencies, effect)` stages work until a successful present; cleanup
runs before a replacement effect and when the component unmounts.

## Store

Store types implement `StoreUnit` and create pure data from Application resources. `use_store`
subscribes a component to a typed selector, and `update_store` or a bound `StoreAction` mutates the
data. The runtime compares selector values and wakes the Application automatically.

```rust,ignore
let count = cx.use_store::<CounterStore, _, _>(|store| store.count);
let increment = cx.store_action::<CounterStore, _>(CounterStore::increment);
```

Store methods return business values, never renderer, window, element, mutation, subscription, or
invalidation types.

## Router

`create_router` defines a retained route tree. Routers use `Location`, `route`, `layout`,
`scope`, `index`, and `outlet`; child patterns are relative to their parent. Path parameters use
`:name`, a final `*name` captures a remaining path, and `RouteMatchHooks` exposes the decoded
parameters and complete match chain.

```rust,ignore
create_router((
    layout(auth_layout, (
        route("/", login_page),
        route("/register", register_page),
    )),
    layout(app_layout, (
        route("/match", match_page),
        scope("/community", (
            index(community_feed),
            route("events", community_events),
            route("articles/:article_id", community_article),
        )),
        not_found(not_found_page),
    )),
))
.outlet(cx)
```

Layout components place `outlet()` where their selected descendant should render. Each Outlet is
a retained component boundary, so changing a leaf preserves ancestor State and Effects. Route
matching ranks complete branches by specificity; static segments beat parameters and wildcards
regardless of declaration order.

`Route::handle` attaches opaque application metadata. `RouteMatches::deepest_handle` reads the
nearest value with parent inheritance, allowing an application layout to select navigation UI
without lgui depending on business types. `Location` history preserves paths, dynamic parameters,
queries, and fragments. `RouteMatches::resolve` performs route-tree-relative resolution.

`route(pattern, component)` has one path-based meaning; the old exact-value flat router no longer
exists. Route lifecycle side effects belong in component Effects.

## Rendering And Resources

Components produce backend-neutral Elements and Scenes. Skia renders the Scene
contract. `RenderResources` carries application-provided asset resolution and custom paint data;
it is supplied through `Application::provide` and scoped to the window render operation. Public
font, icon, image-cache, render-cache, clipboard, desktop, notification, tray, and diagnostics
APIs do not expose Win32 types.

Use `compositing_layer(rect, CompositingLayerSpec::new())` when a subtree must retain and update
its paint independently from siblings. Child coordinates remain declarative window coordinates;
scene compilation converts them to layer-local coordinates. `.opaque()` gives the surface an
opaque black base and enables copy composition, while `.transparent()` preserves alpha and is the
default. `.opacity(value)` controls composition opacity without invalidating retained content.
`.rotation_degrees(value)`, `.rotation_radians(value)`, `.scale(value)`, `.scale_xy(x, y)`,
`.translation(x, y)`, and `.transform_origin(x, y)` apply a transform while the retained surface
is composited. Changing only these values reuses the existing surface pixels. Translation is in
logical pixels and is projected through the current DPI scale.

Every `Element`, `UiElement`, and `UiNode` accepts `.shadow(ShadowStyle)`:

```rust,ignore
group(bounds)
    .content((background, avatar, name))
    .shadow(ShadowStyle::new(Color::BLACK).alpha(72).offset(0.0, 4.0).blur(8.0).spread(0.0))
```

The shadow follows the alpha of the complete element and its subtree, including circles,
paths, glyphs and transparent images supported by the selected renderer. Overlapping children
produce one combined shadow. A child's own explicit shadow is included in its parent's content.
Internal clips shape the source; ancestor clips constrain both content and shadow. Popup
descendants retain their existing escape behavior.

Offsets, blur and spread use logical pixels and follow DPI scaling. Blur is a Gaussian standard
deviation, not a corner radius. Positive spread dilates the alpha mask and negative spread erodes
it (a square kernel rounded outward to physical pixels). An alpha of zero disables the effect.
Layout and hit bounds are unchanged; scene paint bounds and dirty regions include filter padding.

Shadowed subtrees use transparent retained surfaces. Skia applies the same alpha
filter. The filtered result is cached and reused when the subtree moves. Changing the source or filter
rebuilds that surface, so large, continuously changing shadowed groups can be expensive.

Do not use a full-window compositing layer only to preserve z-order. A regular `group` already
keeps retained scene commands in order, and dirty-region rendering replays only commands that
intersect the changed rectangles. Reserve compositing surfaces for content that must be
transformed, faded, or rerasterized independently; their backing storage scales with layer area.

Use `animated_compositing_layer::<T>(rect, configure)` with an application-owned
`CompositingLayerAnimation` state when a layer changes every frame. `lgui` advances the state,
requests frames at its declared interval, and applies its `CompositingLayerSpec` directly to the
retained node without reexecuting the component or rebuilding static children. Layers may be
nested, and popup-phase descendants automatically escape a regular layer.

Retained `HostTree` snapshots share unchanged node storage and keep an indexed node lookup table.
Mounting, layout, focus/animation synchronization, Host commits, and retained Scene reconciliation
therefore operate on the projection change set instead of copying or scanning every node for a
composition-only frame. Runtime diagnostics expose `host_visited_nodes`, `scene_compiled_nodes`,
the individual focus/animation sync and rebuild timings, and the Host change-scan, node-patch,
Scene-reconcile, Scene-snapshot, damage, and finalize timings.

Diagnostics snapshots also include the complete `MemorySnapshot`: per-domain usage and owner,
transient reservations, in-flight large tasks, pinned overflow, and the latest Trim. The Liuguang
debug runtime exposes the same data through `memory.snapshot`; `memory.trim` accepts `memory`,
`persistent`, or `all-rebuildable` scope.

## Async Work

`cx.spawn` and event-context task APIs accept ordinary Futures through the configured `UiExecutor`.
The runtime owns cancellation, UI wake, and delivery to the correct Application. The optional
`tokio` feature is an executor adapter, not a business network runtime.
