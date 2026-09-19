# Blur Design Draft

Status: temporary design note, not yet an implementation contract.

This document records the intended blur semantics and backend ownership before the existing blur
implementation is refactored. It exists to keep the next implementation pass focused and may be
replaced by the relevant architecture and API documentation after the design is implemented.

The desktop stack is Skia-only after the removal of the GDI and Direct2D renderers; all blur work
below is scoped to the single Skia backend.

Implementation status (2026):

- `BackdropBlurStyle` was replaced by the portable `BlurStyle` (`sigma_x`, `sigma_y`, `edge_mode`,
  `opacity`, `tint`, `tint_alpha`); the misleading `source`/`fit`/`source_rect`/`radius` fields
  were removed.
- `BackdropBlur`/`BackdropBlurPath` now sample the accumulated render target via
  `SkCanvas::SaveLayerRec::backdrop` (no image source).
- A new `ContentBlur` primitive and `UiNodeKind::ContentBlur`/`Element::content_blur` were added;
  the Skia backend rasterizes the subtree and blurs it with an image filter.
- `Image` primitives accept an optional `blur: BlurStyle`.
- `examples/backdrop.rs` demonstrates all three effects; `lgui-render-skia` gained two backdrop
  conformance tests (`backdrop_blur_samples_pixels_behind_and_reacts_to_them`,
  `backdrop_blur_softens_edges_and_respects_opacity`).

## Decision Summary

- Blur is a renderer effect. It is not an image asset capability and does not belong to a window
  platform crate.
- An image is one possible input to blur. Rendered content, a retained layer, an alpha mask, and
  the pixels already drawn behind an element are other possible inputs.
- `lgui-core` owns portable effect semantics and Scene representation. Concrete renderers own the
  algorithms and native resources used to implement those semantics.
- Skia implements blur with its image-filter, mask-filter, and layer mechanisms as appropriate.
  In Skia, "image filter" means a filter over rasterized pixels; it is not limited to image assets.
- The public blur amount should use Gaussian standard deviation (`sigma`) in logical units. Each
  backend must project it consistently for the active scale factor.

## Effect Semantics

The blur operation is the same mathematical family, but its input and composition point determine
the visible behavior.

| Effect | Input | Typical use |
| --- | --- | --- |
| Image blur | A specified image resource | Softened artwork or photo processing |
| Content blur | An element or retained subtree rendered to an intermediate layer | Blurring a UI object and all of its children |
| Backdrop blur | Pixels produced by earlier commands behind the element | Acrylic, frosted glass, translucent panels |
| Mask blur | An alpha or coverage mask | Shadows, glows, softened edges |

These must remain distinct in the Scene model. In particular, true backdrop blur does not have an
image source. It samples the accumulated render target at the point where the backdrop command
appears, filters an expanded region, and composites the filtered result through the requested
clip before the element's foreground content is drawn.

The conceptual order is:

```text
draw earlier background commands
    -> capture the backdrop region plus blur outset
    -> apply blur
    -> clip and composite the result
    -> apply optional tint/opacity
    -> draw the element's foreground content
```

## Portable Model

The exact Rust API remains provisional. The portable contract should express an effect separately
from the target to which it is applied. A possible starting point is:

```rust
pub enum RenderEffect {
    GaussianBlur {
        sigma_x: f32,
        sigma_y: f32,
        edge_mode: BlurEdgeMode,
    },
}

pub enum EffectTarget {
    Content,
    Backdrop,
}
```

Image primitives may accept a `RenderEffect` chain, content blur may introduce a filtered-layer
Scene primitive, and backdrop blur may remain a dedicated Scene primitive because it depends on
command ordering and the current render target. Mask blur used by shadows can stay an internal
shadow operation until a public mask-effect API is needed.

Tint and opacity are composition properties rather than part of the blur kernel. They may remain
convenient fields on a backdrop style, but implementations should apply them after filtering so
that every backend follows the same order.

## Parameter Contract

- `sigma_x` and `sigma_y` are Gaussian standard deviations in logical UI units.
- Negative or non-finite values are invalid. Zero disables blur on that axis.
- Scene projection converts logical sigma to the target coordinate space consistently with
  geometry projection.
- Damage and temporary-surface bounds must include the blur outset. A Gaussian implementation
  normally needs approximately `3 * sigma` on each affected side.
- Edge behavior must be explicit. The initial portable modes should cover transparent outside
  pixels and clamped edge pixels; renderer-specific modes must not silently change semantics.
- Backends may use a separable Gaussian, repeated box blur, downsampling, or a native optimized
  implementation, provided conformance images stay within the accepted tolerance.

The current `radius` field is ambiguous: Skia treats it as sigma while the historical CPU path
treated it as a box-kernel radius. The refactor must not preserve that mismatch under a new name.

## Ownership

```text
lgui-core
    portable effect types, Scene commands, ordering, projection, damage

lgui-render-skia
    SkImageFilter/SkMaskFilter and filtered-layer integration, backdrop sampling, Skia caches

lgui-assets
    image loading, decoding, identity, and lifetime; no blur algorithm
```

Shared conformance fixtures can live outside the renderer. Production blur algorithms and native
effect objects must remain owned by the renderer that executes them. With a single Skia renderer
there is no shared CPU helper; a raster utility may exist inside `lgui-render-skia` but must not
be placed in `lgui-platform-win32` or `lgui-assets`.

## Backend Notes

### Skia

Skia's `SkImageFilter` filters rasterized drawing output. It can therefore process a decoded image,
the output of shapes and text drawn into a saved layer, or another filter's output. Image assets do
not own this capability.

The Skia backend should select the appropriate filter/layer path for each target:

- image blur uses an image filter over the decoded image;
- content blur rasterizes the subtree into an offscreen layer and filters that layer;
- backdrop blur samples the accumulated render target behind the element (or an intermediate
  surface capturing the background), filters an outset-expanded region, and composites it through
  the clip.

The active Skia surface decides whether work runs through a GPU implementation or the raster
pipeline; the public effect semantics remain identical across drivers.

## Current Implementation Mismatch

The current `BackdropBlurStyle` contains `source`, `fit`, `source_rect`, and `radius`. The Skia
backend loads that source image and blurs its pixels:

- `draw_backdrop` resolves the image from `UiImageSource::Static(style.source)`, installs
  `image_filters::blur((radius, radius), Clamp)` on the paint, and draws that image clipped to the
  element rect or path, then applies tint.

This behavior is image blur or background-image blur, not backdrop blur. The `radius` value is
passed directly to Skia as a Gaussian sigma even though the field name suggests a radius. The
former GDI and Direct2D backends and their shared CPU blur helper under
`lgui-platform-win32/src/render_support/blur.rs` were removed in the Skia-only refactor; this
Skia path is the only remaining implementation.

## Migration Plan

1. Inventory all callers of `BackdropBlurStyle` and determine whether each caller expects a fixed
   source image or the pixels behind the element.
2. Preserve the existing source-image behavior under an accurately named image/effect API so that
   applications can migrate without an accidental visual change.
3. Introduce the portable Gaussian blur parameter contract and normalize sigma, scale projection,
   edge handling, tint order, opacity, and damage expansion.
4. Add true backdrop and filtered-content Scene semantics with explicit ordering and clip bounds.
5. Update the Skia backend to sample the accumulated render target or an intermediate layer for
   backdrop, and to use filtered layers for content blur, rather than requiring a static source
   image.
6. Remove the old misleading types and compatibility path after all internal callers migrate.
7. Add conformance scenes for image, content, backdrop, and mask blur at multiple scale factors,
   sigma values, edge modes, clips, opacity values, and moving backgrounds, run across the Skia
   GPU and software drivers.
8. Update `API.md`, `ARCHITECTURE.md`, crate READMEs, and examples, then retire this draft.

## Acceptance Criteria

- No public type named backdrop blur requires an image source.
- Blur ownership does not reside in `lgui-assets` or the platform crates.
- The Skia GPU and software drivers render the same Scene effect with comparable bounds and
  intensity.
- Backdrop output changes when an earlier intersecting Scene command changes, including when the
  backdrop element and its style are otherwise unchanged.
- Cache keys include every dependency that can affect filtered pixels; stale backdrop content is
  never reused solely because style and geometry are unchanged.
- Dirty-region expansion prevents clipped blur edges and repaints dependent backdrop regions when
  the sampled background changes.
- Zero sigma has a defined fast path, and invalid values cannot enter backend-native APIs.
- Blur caches report and obey the renderer's assigned memory budget.

## Open Questions

- Whether the first public API exposes only Gaussian blur or a general ordered effect chain.
- Whether image effects are fields on image primitives or represented by a general filtered layer.
- Whether backdrop tint belongs to the backdrop primitive or a following compositing command.
- The exact visual tolerance and reference driver for GPU-vs-software conformance tests.
