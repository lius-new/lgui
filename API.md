# LGUI API

## Application And Windows

`Application::new().window_options(...).run(root)` creates the main window and mounts the root
component. `.provide(value)` adds Application-scoped typed data. `.renderer(RendererKind)` selects
GDI or Direct2D. Optional `.tray(...)`, `.notifications(...)`, and `.executor(...)` configure
library-owned services.

Components access `cx.application()`, `cx.windows()`, and event-context `cx.window()` handles.
Auxiliary windows are declared with `WindowOptions`; `.owner(id)` declares an explicit owner.
Business code never receives native handles.

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

## Async Work

`cx.spawn` and event-context task APIs accept ordinary Futures through the configured `UiExecutor`.
The runtime owns cancellation, UI wake, and delivery to the correct Application. The optional
`tokio` feature is an executor adapter, not a business network runtime.
