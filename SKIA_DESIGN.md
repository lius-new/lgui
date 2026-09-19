# LGUI Cross-Platform Skia Design

Status: implementation complete for the portable Scene, unified text system, software renderer,
winit desktop backend, and OpenGL, Vulkan, and Metal drivers. GDI and Direct2D renderers have been
removed; Skia is now the sole renderer backend. Native Linux/macOS workflow validation remains
acceptance work.

This document is both the implementation contract and the current status record for making
`lgui` a desktop-first, cross-platform Rust GUI framework with Skia as its primary renderer.

"Cross-platform" means that the same Rust UI, retained tree, layout, input model, and Scene run
on Windows, Linux, and macOS. It does not mean exposing `lgui` to other programming languages.
A stable C ABI or language bindings are a separate future project.

## Decision Summary

- Preserve the existing component runtime, retained Host tree, incremental Scene commits,
  dirty-region rendering, and retained compositing layers.
- Introduce one platform-neutral renderer lifecycle before adding Skia. GDI and Direct2D must
  implement that lifecycle during migration; no parallel legacy renderer contract remains.
- Use Skia for paths, text, images, SVG, effects, clipping, and layer rasterization.
- Use `winit` for the portable desktop event loop and top-level windows.
- Use `raw-window-handle` at the window/surface boundary and AccessKit for accessibility.
- Support Skia GPU drivers plus a software driver. Driver selection and fallback are explicit and
  observable.
- Keep Windows-only capabilities such as AUMID, Toast registration, and native tray behavior in a
  Windows extension crate, outside the portable runtime.
- Keep the local `freya/` checkout as design reference only. `lgui` does not depend on or vendor it.
- Keep business behavior in application crates. `lgui` provides mechanisms and capabilities only.

## Goals

1. Run the same declarative UI and state model on Windows, Linux, and macOS.
2. Render every portable Scene primitive through Skia with visual and interaction parity.
3. Retain incremental work: a composition-only animation must not rebuild the component tree,
   walk unrelated Host nodes, rerasterize unchanged content, or repaint the full window.
4. Use the same text engine for measurement, painting, caret placement, selection, and hit testing.
5. Provide complete keyboard, pointer, touch, wheel, IME, clipboard, DPI, theme, and accessibility
   integration on supported desktop platforms.
6. Make GPU/device loss, fallback, cache budgets, and memory pressure deterministic and observable.
7. Keep `lgui` usable without a desktop backend or Skia through a portable, testable core.
8. Preserve a single public Application model for main windows, auxiliary windows, Stores, Router,
   effects, tasks, and render diagnostics.

## Non-Goals

- Web rendering or WebAssembly in the first implementation.
- Android or iOS in the desktop acceptance gate. Android may follow after the desktop contracts
  stabilize.
- A foreign-language ABI, widget protocol, or serialization format.
- Adopting Freya's component model, layout engine, or full-frame render pipeline.
- Exposing Skia types from `lgui-core` or requiring ordinary components to know the active backend.
- Emulating every platform-specific GDI or Direct2D rasterization quirk.
- Moving application pages, stores, routes, networking, or game-specific animation state into
  `lgui`.

## Current Implementation

The implementation currently provides:

- a backend-neutral retained Host tree, incremental Scene, logical floating-point geometry,
  outward-rounded physical damage, and renderer lifecycle;
- real Skia paint paths for all 17 `ScenePrimitive` variants, including nested clips, custom
  `SceneFragment` content, static/compositing layers, scroll raster content, images, SVG, blur,
  overlays, opacity, and DPI projection;
- one SkParagraph text path for measurement and painting with bidi layout, grapheme clusters,
  caret affinity, selection rectangles, hit testing, wrapping, spans, locale and runtime fonts;
- renderer-scoped, byte-bounded decoded/layer caches with hit, miss, eviction, resident-byte, and
  budget diagnostics plus moderate and critical memory-pressure handling; GPU renderers split one
  configured budget between native GPU resources and renderer-owned CPU resources;
- a software Skia renderer presented through `softbuffer` and a portable OpenGL renderer using
  `glutin`, `raw-window-handle`, and the same Scene painter;
- a winit application backend with multi-window ownership, visibility/close policy, resize,
  fullscreen, DPI, keyboard, pointer, wheel, touch, file drop, theme, IME, and bounded renderer
  recovery;
- a backend-neutral semantics tree mapped to AccessKit, including action dispatch and focus
  synchronization without visual damage for semantics-only updates;
- Application-scoped portable clipboard, URL opening, file-dialog, image-loading, and render
  resource capabilities; File and URL image I/O runs off the frame thread and wakes the app when
  bytes are ready;
- Windows-only AUMID, Toast, tray, owner HWND, and native window policy kept in the Win32 adapter;
- application-owned custom painting: Liuguang implements `CustomPaintProvider` in
  `native/windows` and records portable Scene commands. The Skia renderer contains no Liuguang
  paint keys or page behavior.

The remaining acceptance gaps are explicit:

- Vulkan and Metal surface drivers are implemented behind explicit features. Native Vulkan
  workflow validation on Windows/Linux and Metal compile/runtime validation on macOS remain open.
- Linux runtime validation and macOS compile/runtime validation require their native CI or
  hardware. Windows compilation and headless Scene conformance are covered locally.
- GDI/D2D remain the default migration backends and are removed only after Skia workflow and
  performance acceptance on all three desktop platforms.

## Target Matrix

| Platform | Window backend | Current Skia GPU driver | Fallback | Validation status |
| --- | --- | --- | --- | --- |
| Windows | winit | OpenGL or Vulkan | software | OpenGL compiles locally; Vulkan runtime validation pending |
| Linux | winit (Wayland/X11) | Vulkan, then OpenGL | software | Source paths implemented; native runtime pending |
| macOS | winit | Metal, then OpenGL | software | Source paths implemented; native compile/runtime pending |
| Android | not in the desktop backend | none | none | Deferred |

`Auto` follows the platform order in the table and falls back to software when GPU initialization
fails or after a bounded recovery sequence. An explicit unavailable driver returns an error and
never silently falls back. Driver policy is framework configuration, not compile-time business
logic.

## Dependency Policy

`lgui-render-skia` uses the upstream `skia-safe` crate from `rust-skia`, initially pinned as
`skia-safe = "=0.99.0"`. It does not depend on `freya-skia-safe` or the local Freya checkout.
The Freya fork is useful implementation evidence, but coupling the renderer to a framework-owned
fork would transfer its release policy and patch cadence into `lgui`.

The dependency uses the exact upstream 0.99.0 Windows binary-cache feature set already published
by rust-skia: `binary-cache`, `embed-icudtl`, `gl`, `jpeg`, `pdf`, `svg`, and `textlayout`.
Keeping this set exact avoids an implicit local LLVM/Ninja Skia source build. `renderer-skia` owns
the portable painter and software surface; `renderer-skia-gl` adds the winit/glutin OpenGL surface
adapter. Platform cfg selects WGL on Windows, EGL on Linux, and CGL on macOS.

Every upgrade records the Skia milestone, binding version, enabled features, binary-cache source,
license review, supported Rust version, release-size change, and conformance result. If an upstream
defect blocks the framework, the preferred resolution is an upstream patch. Any temporary fork
must be owned and pinned by this project with a removal condition; switching silently to a
third-party framework fork is not allowed.

## Architecture

```text
lgui facade
    |---> lgui-core
    |---> lgui-render-skia ---> lgui-render-api ---> lgui-core
    |---> lgui-platform-winit -> lgui-render-api ---> lgui-core
    +---> lgui-platform-win32 -> lgui-render-api ---> lgui-core
```

An arrow means "depends on". The facade selects and composes concrete implementations; portable
components continue to depend only on the facade/core API.

The dependency rules are:

- `lgui-core` has no dependency on `windows`, `winit`, `raw-window-handle`, Skia, OpenGL, Vulkan,
  Metal, Direct2D, or GDI.
- `lgui-render-api` depends only on `lgui-core` and defines frame, renderer, damage, resource, and
  diagnostics contracts.
- `lgui-render-skia` depends on the Skia bindings, `lgui-core`, and `lgui-render-api`. It does not
  create windows or translate OS input.
- `lgui-platform-winit` owns windows, event dispatch, redraw scheduling, IME placement, DPI,
  AccessKit adapters, and the generic desktop render-runtime boundary. Its optional `skia` module
  binds window/raw handles to Skia OpenGL, Vulkan, Metal, or software surfaces and composes them
  with `lgui-render-skia`. Without that feature the crate does not depend on Skia.
- `lgui-platform-win32` owns the native Win32 application adapter and GDI/Direct2D renderers. It
  consumes `lgui-render-api` and the shared runtime, but does not own a second Host tree.
- The `lgui` package remains the facade that selects features, supplies ergonomic constructors,
  and re-exports stable public APIs.

Separate crates enforce these rules at compile time. The intended workspace layout is:

```text
native/
  lgui/                         facade package and public documentation
    crates/
      core/                     component/runtime/layout/input/Scene/semantics
      render-api/               renderer and surface contracts
      render-skia/              Skia Scene renderer and caches
      platform-winit/           desktop Application backend and optional Skia surface bridge
      platform-win32/           Windows-only capability providers
```

The split is performed as moves, not copies. A source module has one owner throughout migration.

## Portable Core Contract

`lgui-core` owns:

- component identity, hooks, State, Effects, Context, Stores, Router, and tasks;
- retained component and Host trees;
- layout, hit testing, focus, popups, and window-independent event dispatch;
- Scene construction, stable command identity, damage calculation, and compositing specifications;
- portable asset identifiers and revisions;
- semantic accessibility nodes and actions;
- platform-neutral diagnostics emitted before rendering.

The core does not decode assets, select fonts from the operating system, allocate native surfaces,
or present frames.

### Geometry

Layout and input coordinates use logical `f32` device-independent pixels. Physical damage and
surface dimensions use integer pixels.

- Logical rectangles are projected using the current scale factor.
- Damage projection rounds outward so fractional scaling never drops changed pixels.
- Skia receives floating-point physical coordinates and may apply pixel snapping only for
  primitives whose style requests it.
- Hit testing uses logical coordinates and does not depend on renderer rounding.
- Invalid, NaN, or infinite geometry is rejected at construction or normalized to an empty node;
  it never reaches a backend.

This is a deliberate public API break. It must be completed as one migration rather than hidden by
integer compatibility overloads.

### Scene Model

The existing Scene remains a retained display list. It must describe visual intent rather than a
specific graphics API. Stable command IDs, child signatures, content revisions, phase ordering,
and layer-local coordinates remain part of the contract.

Scene data is immutable for a submitted frame and cheap to clone through shared storage. A Scene
must be `Send + Sync` even when a platform executes and renders it on one UI thread. Native Skia,
DirectWrite, GDI, and Direct2D objects never enter Scene values.

## Render Lifecycle

The current `RenderBackend<Target>` method is replaced by one lifecycle used by all renderers:

```rust,ignore
pub trait SceneRenderer {
    type Target;

    fn capabilities(&self) -> RendererCapabilities;
    fn prepare(&mut self, target: &mut Self::Target, frame: &FrameInfo)
        -> Result<(), RenderError>;
    fn render(
        &mut self,
        target: &mut Self::Target,
        scene: &Scene,
        damage: &DamageRegion,
        frame: &FrameInfo,
    ) -> Result<RenderStats, RenderError>;
    fn trim(&mut self, pressure: MemoryPressure);
    fn reset(&mut self);
}
```

`FrameInfo` contains logical size, physical size, scale factor, frame reason, scene revision, and
whether a full redraw is mandatory. `RenderStats` contains actual painted bounds, cache activity,
layer activity, and backend timing. Presentation belongs to the graphics driver, not the Scene
renderer.

A graphics driver owns the native surface and implements this sequence:

```text
acquire -> renderer.prepare -> renderer.render -> flush -> present
```

The winit Application backend owns the driver and renderer for each window. All windows share
Application resources but have independent surface/device state and renderer-scoped caches.

## Frame Scheduling

Frames are event driven. The portable runtime requests a frame only for:

- a committed component/store/router change;
- active animation whose next deadline has arrived;
- explicit application request;
- window expose, resize, DPI, theme, or device recovery;
- asynchronous asset completion;
- accessibility or platform state requiring a visual update.

Multiple requests coalesce into one winit `request_redraw`. On `RedrawRequested`:

1. advance due animations;
2. reconcile dirty components and Host nodes;
3. update focus, semantics, Scene roots, and damage;
4. skip drawing when there is no platform exposure and no damage;
5. acquire a surface frame and render only required regions/layers;
6. flush and present;
7. run post-present effects;
8. schedule the next animation deadline, if any.

The event loop never polls continuously to keep animations alive. Resize/move loops may lower
animation frequency, but input, exposure, and final layout remain responsive.

## Skia Scene Mapping

| Scene primitive | Skia operation | Retained resource |
| --- | --- | --- |
| `Rect`, `Ellipse`, `Line` | Canvas draw operations with Paint | Paint/style cache where useful |
| `Path` | `SkPath` plus fill/stroke Paint | Path keyed by geometry revision |
| `Text` | SkParagraph/TextBlob | Text layout and glyph resources |
| `Custom` | portable SceneFragment or declared Skia extension | revision-keyed compiled output |
| `Image` | SkImage draw with sampling options | decoded image and GPU texture |
| `Icon` | Skia SVG DOM or recorded picture | parsed SVG and raster/GPU resources |
| `Glow` | mask/image filter or bounded layer | filter keyed by radius/color |
| `BackdropBlur`, `BackdropBlurPath` | bounded backdrop filter | filter; no full-window copy |
| `Overlay` | native Skia gradient shader | shader keyed by gradient specification |
| `Clip`, `ClipPath` | Canvas save, clip, restore | cached path where applicable |
| `StaticLayer` | SkSurface snapshot or SkPicture | budgeted layer cache |
| `ScrollRaster` | tiled SkSurface/SkImage cache | visible and bounded prefetch tiles |
| `CompositingLayer` | offscreen SkSurface plus transform/opacity | retained layer surface |

Each primitive must have a conformance test covering full draw, clipped draw, removal damage,
opacity, DPI projection, nested clipping, and cache invalidation.

Skia rendering must preserve the current Scene ordering rules: Background, Content, Overlay, then
Popup. Popup descendants escape ordinary compositing layers exactly as they do today.

## Text System

Text measurement and text painting cannot use separate engines. A `TextSystem` service is created
by the active renderer family and supplied to layout and input code.

The portable request includes:

- font family fallback list, size, weight, width, slant, locale, and features;
- wrapping width, line limit, alignment, direction, and line-height behavior;
- text revision and optional spans;
- scale-independent caret and selection requirements.

The result exposes portable metrics, lines, cluster boundaries, caret locations, selection
rectangles, and hit testing. The backend retains its native paragraph/layout object behind an
opaque renderer-owned ID.

The Skia implementation uses SkParagraph and one renderer-owned `FontCollection`. Measurement and
painting share the same paragraph builder configuration. Applications load font bytes through the
portable `FontAsset` resource, without exposing Skia types to application code.

Public text positions are Rust character indices. SkParagraph range APIs use UTF-16 code-unit
offsets, so the backend converts character, UTF-8 byte, and UTF-16 boundaries explicitly before
line, cluster, caret, selection, or hit-test geometry crosses the renderer boundary. Grapheme
clusters prevent controls from placing a caret inside a combining sequence. Tests cover bidi,
combining marks, caret affinity, selection, and hit testing; installed-system fallback remains a
platform-dependent runtime check.

IME cursor placement is calculated from the same text layout result used for painting.

## Assets and Images

Portable Scene commands reference `AssetId` plus an explicit content revision. They do not contain
Win32 cache types or native image handles.

The asset pipeline is:

```text
AssetResolver -> encoded bytes -> metadata/decode task -> renderer upload -> Scene ready
```

- Resolution and I/O may run asynchronously.
- Decode policy belongs to a portable codec service or the Skia renderer, not to Win32.
- The renderer caches decoded images and GPU textures by asset identity, revision, color space,
  and requested sampling requirements.
- Reachability from live Scenes and a byte budget bound cache lifetime.
- A completed asynchronous load invalidates only nodes that reference the asset.
- SVG source remains encoded/parsed vector data where possible; it is not forced through a
  full-sized intermediate bitmap.
- Color space and alpha type are explicit. The portable default is sRGB with premultiplied alpha
  at the render target.

## Custom Painting

Portable custom painting is implemented through `CustomPaintProvider::record`, which returns a
`SceneFragment` containing ordinary `ScenePrimitive` commands. It supports the same paths, text,
clips, and layers as the retained Scene and works on every renderer. Application custom painting
therefore stays in the application crate and has no Skia or native-window dependency.

There is intentionally no public Skia Canvas escape hatch in the current API. Bitmap output may
still be represented as an ordinary image asset when bitmap generation is actually intended.

## Platform Application Backend

`WinitApplication` implements the existing `ApplicationBackend` ownership contract. It owns one
winit event loop, the portable WindowManager, platform adapters, and all per-window presentation
state.

Portable `WindowOptions` contains title, visibility, logical size constraints, resizability,
transparency, decoration choice, position policy, owner relationship, mode, close policy, and
application icon in a portable decoded/encoded form.

Platform-only settings are typed extensions:

- `Win32WindowOptions`: class name, AUMID-related identity, native corner preference, and other
  HWND-specific policies;
- future Linux and macOS extensions follow the same rule.

Portable code never sees `HWND`, `HDC`, Win32 message IDs, X11 handles, Wayland objects, or AppKit
objects. An advanced platform extension may expose a callback with a borrowed raw handle, scoped
to the window lifetime.

## Input and IME

The portable input model must cover the complete data supplied by winit:

- logical and physical key, key location, repeat, pressed/released state;
- Ctrl, Shift, Alt, Meta/Super, Caps Lock, and Num Lock modifiers;
- pointer ID, device kind, buttons, logical position, pressure where available, enter/leave;
- vertical and horizontal wheel deltas with line/pixel source and touch phase;
- touch start/move/end/cancel;
- IME enabled state, preedit text and cursor range, commit text, and candidate-area rectangle;
- focus, file hover/drop, scale-factor, theme, and window state changes.

`keyboard-types` is the shared keyboard vocabulary unless a required platform event cannot be
represented. Backspace, copy, paste, and navigation are derived by controls from keyboard events;
they are not separate platform-only shortcuts.

Pointer capture, focus scopes, popup dismissal, and drag regions remain core behavior. Platform
capture is used when a drag must continue outside the client area.

## Accessibility

Each Host node may produce a backend-neutral semantic node with role, name, description, value,
state, actions, bounds, relationships, and text ranges. Semantic identity derives from stable
`UiId` identity and is independent from paint command identity.

The core computes semantic diffs alongside Host commits. `lgui-platform-winit` maps those diffs to
AccessKit and maps AccessKit actions back to `UiEvent`s. Focus synchronization has one source of
truth: changing UI focus updates accessibility focus, and an accessibility focus action updates UI
focus.

Semantic updates do not force a visual repaint unless the corresponding UI state changes.

## Platform Capabilities

Clipboard, URL opening, dialogs, notifications, tray, system theme, and platform preferences are
Application-scoped capabilities. Each capability has a platform-neutral handle and explicit
`Unsupported` or `Unavailable` errors.

The baseline desktop backend provides:

- text clipboard on all required desktop platforms;
- URL opening and file dialogs through portable providers;
- theme, accent color where available, cursor, monitors, and DPI;
- notifications and tray where a production-quality platform implementation exists.

Windows AUMID registration, Toast XML, taskbar recreation, and native tray message handling remain
in `lgui-platform-win32`. Other platforms do not compile these dependencies.

## Compositing and Incremental Rendering

The existing retained compositing contract is mandatory for Skia:

- unchanged layer descendants retain their surface or picture;
- transform and opacity-only animation changes composition properties without rerasterization;
- local layer damage redraws only the affected layer region;
- window damage includes old and new transformed bounds;
- layer insertion, removal, resize, content revision, and color-space change invalidate the
  correct resources;
- popup roots are composed after ordinary layers;
- stable frames reuse Scene roots and do not reconstruct paint order.

Skia `saveLayer` is not itself a retained layer. Long-lived `CompositingLayer` and `StaticLayer`
objects use explicitly owned offscreen surfaces, images, or pictures with measured memory cost.

## Cache and Memory Policy

Every cache is renderer-scoped or device-scoped, byte-accounted, reachable from current Scenes,
and bounded by least-recently-used eviction. No process-global native graphics cache is allowed.

Required cache classes are:

- text layouts and typefaces;
- decoded images and GPU textures;
- parsed SVG documents and paths;
- Skia paths/shaders/filters;
- static and compositing layer surfaces;
- scroll tiles.

The renderer exposes configured bytes, resident bytes, live entry count, hits, misses, evictions,
and the largest entries. Cache budgets scale with window surface area and may be capped by
Application configuration. Background memory optimization releases offscreen layers and GPU
resources while preserving portable Scene and state.

The default renderer budget is 96 MiB total. GPU drivers assign two thirds to Skia's native
`DirectContext` resource cache and one third to decoded images, retained layers, and paragraphs;
diagnostics merge both portions so the reported budget and residency describe the complete
renderer rather than two independent 96 MiB caches. The software driver uses the configured
budget entirely for renderer-owned CPU resources.

`MemoryPressure::Moderate` removes cold resources. `Critical` removes every recreatable resource
and forces a full draw on the next visible frame.

## Driver Selection and Recovery

```rust,ignore
pub enum GraphicsPreference {
    Auto,
    OpenGl,
    Vulkan,
    Metal,
    Software,
}
```

An unsupported explicit choice returns a creation error. `Auto` follows the target matrix and
records failed initialization and recovery through the active renderer name, fallback reason,
recovery state, and recovery attempt diagnostics. Each implemented driver reports its available
GPU adapter name, API/version, color format, and present mode.

On recoverable surface loss, the driver recreates only the surface and forces a full draw. On
device loss:

1. stop presenting that window;
2. drop device-owned caches and surfaces;
3. recreate the preferred driver once;
4. fall through the configured `Auto` chain if recreation fails;
5. rebuild renderer resources from portable Scene/assets;
6. submit one full frame;
7. report the complete recovery path.

Recovery is bounded. Repeated failures produce one terminal `RenderError`; they do not create an
infinite redraw loop.

## Features

The implemented feature model is:

```text
default = [renderer-gdi, router, store, widgets]
desktop = [backend-winit, clipboard, open-url, dialogs, accessibility]
backend-winit
renderer-skia
renderer-skia-gl = [renderer-skia, backend-winit, glutin]
renderer-skia-vulkan = [renderer-skia, backend-winit, ash]
renderer-skia-vulkan-windows = [renderer-skia-vulkan, skia-safe/d3d]
renderer-skia-vulkan-linux = [renderer-skia-vulkan, skia-safe/all-linux]
renderer-skia-metal = [renderer-skia, backend-winit, objc2]
renderer-gdi
renderer-d2d
```

Platform target conditions select the legal glutin display API. Features express capability;
`GraphicsPreference` and `--renderer` express runtime choice. `--no-default-features` continues
to build and test the portable core.

During migration the existing default remains unchanged. The default switches to Skia only after
the parity and performance gates pass. GDI/D2D features are then removed in a dedicated breaking
change rather than retained as permanent compatibility paths.

Liuguang launch examples from `native/windows` are:

```powershell
$env:LIUGC_DIAGNOSTICS_RUNTIME='1'
cargo run --bin liugc --features diagnostics-runtime,renderer-skia -- --renderer skia
cargo run --bin liugc --features diagnostics-runtime,renderer-skia -- --renderer skia-opengl
cargo run --bin liugc --features diagnostics-runtime,renderer-skia -- --renderer skia-vulkan
cargo run --bin liugc --features diagnostics-runtime,renderer-skia -- --renderer skia-software
```

`skia` and `skia-auto` select `GraphicsPreference::Auto`. Explicit `skia-opengl`, `skia-vulkan`,
and `skia-metal` selections fail if that driver cannot be created; `skia-software` never creates a
GPU context. Auto uses OpenGL on Windows, Vulkan then OpenGL on Linux, and Metal then OpenGL on
macOS before the software fallback.

The `counter` example is application-independent and uses the same winit + Skia source on every
desktop platform:

```powershell
cargo run --manifest-path native\lgui\Cargo.toml --example counter --no-default-features --features renderer-skia-gl,widgets
```

## Diagnostics

Existing runtime diagnostics include:

- active renderer (`skia-opengl`, `skia-vulkan`, `skia-metal`, or `skia-software`) and fallback
  reason;
- frame reason and full/dirty/skipped classification;
- Host visited nodes, Scene compiled nodes, command count, and layer count;
- logical and physical damage area and rectangle count;
- frame build, acquire, draw, flush/submit, present, and combined draw/present timings;
- adapter, graphics API/version, color format, and present mode metadata;
- image/layer and paragraph cache budgets, resident bytes, entries, hits, misses, evictions, and
  largest-entry sizes, including native GPU cache usage;
- device/surface recovery attempts and fallback reason.

Diagnostics must remain optional and must not add per-command allocations when disabled.

## Migration Plan

### Phase 0: Contract Freeze

Status: complete for the current monolithic crate boundary.

- Approve this design and record unresolved decisions as explicit amendments.
- Add architecture tests for the target dependency direction.
- Add a Scene primitive conformance inventory and current GDI/D2D screenshots/diagnostics baselines.

Exit gate: every current primitive and platform capability has one named target owner.

### Phase 1: Portable Boundaries

Status: functionally complete as enforced module boundaries; physical crate extraction remains an
optional packaging change and is not required by the public API.

- Move runtime/Scene code into `lgui-core` and renderer contracts into `lgui-render-api`.
- Replace Win32 image, text, cache, and renderer types in public APIs.
- Introduce logical floating-point geometry and physical outward-rounded damage.
- Expand portable input and semantic accessibility models.
- Move GDI/D2D onto the new renderer lifecycle.

Exit gate: GDI/D2D behavior remains green, `lgui-core` builds on Windows/Linux/macOS targets, and
architecture tests find no platform or graphics dependencies in portable crates.

### Phase 2: Complete Software Skia Renderer

Status: complete for the current 17-primitive Scene inventory and focused conformance suite.

- Implement all Scene primitives, text, image, SVG, clip, effects, static layers, compositing
  layers, and scroll tiles.
- Implement cache budgets and software surfaces.
- Add deterministic bundled-font headless rendering and golden/conformance tests.

Exit gate: no Scene primitive falls back to a placeholder or bitmap-only compatibility adapter.
Headless parity, damage, cache invalidation, and memory-budget tests pass.

### Phase 3: Winit Desktop Backend

Status: implemented; Windows compiles locally, while Linux and macOS native workflow validation
remain acceptance requirements.

- Implement Application lifecycle, multi-window ownership, redraw scheduling, input, IME, DPI,
  clipboard, URL opening, dialogs, theme, cursor, file drop, and AccessKit.
- Integrate the software Skia renderer as a guaranteed fallback.
- Implement Windows extension capabilities required by Liuguang.

Exit gate: showcase and UI lab complete their interaction suites through winit on Windows and
Linux; macOS compiles and passes platform-independent tests. No Win32 event loop is used by the
winit path.

### Phase 4: GPU Drivers

Status: implementation complete; native driver conformance and product acceptance remain open.

- Add OpenGL, Vulkan, and Metal surface drivers according to the target matrix.
- Implement resize, transparency, vsync/present mode, cache limits, device loss, and fallback.
- Run Scene conformance through every available driver.

Exit gate: Windows OpenGL/software, Linux Vulkan/OpenGL/software, and macOS Metal/software pass
the same renderer suite. Explicit driver selection and bounded recovery are verified.

### Phase 5: Liuguang Migration

Status: implemented behind the `renderer-skia` client feature and explicit `--renderer` switch;
full workflow and performance acceptance is user-run.

- Run the existing frontend unchanged where it already uses portable APIs.
- Replace remaining application calls to Win32-only GUI services with typed capabilities.
- Validate login, lobby, Store, Settings, auxiliary windows, tray, notifications, Steam/WebView
  integration, text input, DPI, and background behavior.
- Compare CPU, GPU, frame latency, memory, and cache diagnostics with the fixed GDI/D2D baseline.

Exit gate: all named workflows pass and the lobby animation keeps the retained composition
behavior already validated on D2D. No business component contains Skia or winit types.

### Phase 6: Default Switch and Removal

Status: pending cross-platform and product acceptance.

- Make winit + Skia the default desktop path.
- Remove the replaced Win32 Application renderer path, GDI/D2D renderer features, native caches,
  and compatibility-only tests/docs.
- Keep only the Windows capability extension code still required by applications.

Exit gate: repository scans find no replaced renderer/runtime path, the complete acceptance matrix
passes, and release artifacts contain only the selected platform drivers.

## Validation Matrix

Every phase runs the existing `lgui` and frontend suite plus its new focused tests.

Required automated validation at completion:

- portable core tests with no default features;
- architecture dependency tests;
- retained Host/Scene incremental tests;
- renderer conformance and software golden tests;
- input, IME, focus, and accessibility semantic tests;
- image/SVG/text/cache/device-recovery tests;
- multi-window ownership and resource-release tests;
- Windows and Linux compile/runtime suites;
- macOS compile and runtime suite on macOS CI/hardware;
- formatting, documentation examples, and diff checks.

Required runtime scenarios use a fixed build profile, viewport, DPI, page, input sequence, and
sample duration. They record average and P95 frame time, CPU, GPU, working set, private bytes,
cache bytes, dirty ratio, Host visits, Scene compilations, and layer reuse.

No universal FPS number is an acceptance threshold. The required properties are bounded memory,
no frame-loop work while idle, no full-tree or full-window work for composition-only animation,
and no material regression against the same-platform baseline without an approved explanation.

## Acceptance Criteria

The project is cross-platform only when all of the following are true:

1. A nontrivial shared showcase runs on Windows, Linux, and macOS from the same component code.
2. The complete portable Scene is rendered by Skia without placeholder primitives.
3. Text measurement, paint, selection, caret, and IME placement agree.
4. Keyboard, pointer, wheel, touch where available, clipboard, DPI, theme, file drop, and
   accessibility work through portable contracts.
5. Multi-window ownership, visibility, resize, close, and resource release pass.
6. GPU and software paths use the same Scene and pass the same conformance suite.
7. Driver loss and fallback are bounded and recover portable state.
8. Idle windows do not schedule continuous frames.
9. Composition-only animation retains unchanged Host nodes, Scene roots, and layer content.
10. Renderer caches are byte-bounded, observable, and released with their owning renderer/device.
11. Application/frontend code has no direct dependency on Skia, winit, or native window handles.
12. Replaced GDI/D2D runtime and renderer paths are removed after the default switch.

## Principal Risks

### Build size and time

Skia bindings increase dependency build time and artifact size. Pin one binding version, use
prebuilt binaries where reproducible, isolate driver features, cache CI artifacts, and measure
release size before changing defaults.

### Text parity

Font discovery and fallback differ across systems. One Skia text system prevents measure/paint
drift, while bundled test fonts keep conformance deterministic. Product typography still requires
runtime validation on every platform.

### GPU driver variability

OpenGL and Vulkan behavior varies by driver. `Auto` has a bounded fallback chain and software is a
real renderer, not a diagnostic stub.

### Transparency and native integration

Transparent windows, WebView composition, trays, notifications, and owner-window semantics are
not supplied by Skia. They have explicit platform capability owners and dedicated workflow tests.

### Accidental full-frame rendering

Skia makes full-frame drawing easy but does not make it free. Damage, retained layers, stable Scene
roots, and frame diagnostics remain mandatory parts of the renderer contract.

### API leakage

Allowing Skia Canvas, winit events, or raw handles into ordinary components would make the public
framework nominally rather than genuinely portable. Architecture tests reject those dependencies;
advanced extensions stay in backend-specific crates.

## Design Completion Rule

Implementation may be delivered in the phases above, but no phase may leave duplicate ownership,
placeholder primitives, silent renderer fallback, unbounded caches, or compatibility adapters
described as future cleanup. A phase is complete only when its exit gate and focused validation
pass. The final cross-platform claim is reserved for the full acceptance criteria, not for merely
opening a Skia window on Windows.
