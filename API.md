# LGUI API

## Application And Windows

`Application::new().window_options(...).run(root)` creates the main window and mounts the root
component. `.provide(value)` adds Application-scoped typed data. `.renderer(RendererKind)` selects
GDI or Direct2D. Optional `.tray(...)`, `.notifications(...)`, and `.executor(...)` configure
library-owned services.

Components access `cx.application()`, `cx.windows()`, and event-context `cx.window()` handles.
Auxiliary windows are declared with `WindowOptions`; `.owner(id)` declares an explicit owner.
Business code never receives native handles.

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

Application::new()
    .command::<Login>(move |_cx, request| {
        let auth = auth.clone();
        async move { auth.login(request).await }
    })
    .executor(executor)
    .run(app)?;
```

Async UI handlers receive an owned `UiAsyncContext`, so they can await without retaining the
synchronous input-dispatch borrow. The configured Application executor runs the handler.

```rust,ignore
button(rect, "Login", style).on_click_async(move |ui| {
    let request = request.clone();
    let set_session = set_session.clone();
    async move {
        set_session(ui.invoke::<Login>(request).await.ok());
    }
})
```

Controlled widgets with value callbacks use the same adapter without a Store requirement:

```rust,ignore
checkbox(rect, checked, async_handler_with(move |ui, checked| async move {
    ui.invoke::<SetPreference>(checked).await.ok();
}))
```

Effects can retain a cloneable command handle and await it independently of Store:

```rust,ignore
let load_profile = cx.command::<LoadProfile>();
cx.use_async_effect(user_id.clone(), move || async move {
    set_profile(load_profile.invoke(user_id).await.ok());
});
```

Events are typed Application broadcasts. `emit` is synchronous and returns the number of current
listeners. `use_event` and `use_event_async` subscriptions are Effect-owned and unsubscribe when
their dependencies change or their component unmounts.

```rust,ignore
#[derive(Clone)]
struct DownloadProgress { received: u64, total: u64 }

impl Event for DownloadProgress {
    const NAME: &'static str = "download.progress";
}

cx.use_event::<DownloadProgress>((), move |progress| {
    set_progress(progress.received as f32 / progress.total as f32);
});

ui.emit(DownloadProgress { received, total });
```

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

Components produce backend-neutral Elements and Scenes. GDI and Direct2D render the same Scene
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

Do not use a full-window compositing layer only to preserve z-order. A regular `group` already
keeps retained scene commands in order, and dirty-region rendering replays only commands that
intersect the changed rectangles. Reserve compositing surfaces for content that must be
transformed, faded, or rerasterized independently; their backing storage scales with layer area.

Direct2D keeps reusable image, icon, blur, and static-layer bitmaps only while their cache keys are
reachable from the current Scene and within the backend cache budget. A transparent static layer
whose only content is a baked image reuses that image bitmap instead of allocating a second
same-sized render target. Overlay gradients are drawn with native Direct2D brushes, and their
brush resources are reused while the matching overlay remains on the active Scene.

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

## Async Work

`cx.spawn` and event-context task APIs accept ordinary Futures through the configured `UiExecutor`.
The runtime owns cancellation, UI wake, and delivery to the correct Application. The optional
`tokio` feature is an executor adapter, not a business network runtime.
