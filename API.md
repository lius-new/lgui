# LGUI API

## Application And Windows

`Application::new().memory_options(...).window_options(...).run(root)` creates the main window and
mounts the root component. `.provide(value)` adds Application-scoped typed data. `.renderer(RendererKind)` selects
GDI or Direct2D. Optional `.tray(...)` and `.notifications(...)` configure the built-in Windows
adapters when `tray-win32` and `notifications-win32` are enabled. `.notification_service(...)`
installs a portable application-provided adapter, and `.executor(...)` configures task execution.

Components access `cx.application()`, `cx.windows()`, and event-context `cx.window()` handles.
Auxiliary windows are declared with `WindowOptions`; `.owner(id)` declares an explicit owner.
Business code never receives native handles.

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

Application::new()
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

Application::new()
    .memory_options(application_memory_options())
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

Diagnostics snapshots also include the complete `MemorySnapshot`: per-domain usage and owner,
transient reservations, in-flight large tasks, pinned overflow, and the latest Trim. The Liuguang
debug runtime exposes the same data through `memory.snapshot`; `memory.trim` accepts `memory`,
`persistent`, or `all-rebuildable` scope.

## Async Work

`cx.spawn` and event-context task APIs accept ordinary Futures through the configured `UiExecutor`.
The runtime owns cancellation, UI wake, and delivery to the correct Application. The optional
`tokio` feature is an executor adapter, not a business network runtime.
