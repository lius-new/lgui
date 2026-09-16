# Blur Design Draft

Status: temporary design note, not yet an implementation contract.

This document records the intended blur semantics and backend ownership before the existing blur
implementation is refactored. It exists to keep the next implementation pass focused and may be
replaced by the relevant architecture and API documentation after the design is implemented.

## Decision Summary

- Blur is a renderer effect. It is not an image asset capability and does not belong to a window
  platform crate.
- An image is one possible input to blur. Rendered content, a retained layer, an alpha mask, and
  the pixels already drawn behind an element are other possible inputs.
- `lgui-core` owns portable effect semantics and Scene representation. Concrete renderers own the
  algorithms and native resources used to implement those semantics.
- GDI implements blur with renderer-owned offscreen BGRA buffers and a CPU algorithm.
- Direct2D implements blur with `ID2D1Effect` and `CLSID_D2D1GaussianBlur` when the input is
  available as a Direct2D image or intermediate surface.
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

The current `radius` field is ambiguous: Skia treats it as sigma while the CPU path treats it as a
box-kernel radius. The refactor must not preserve that mismatch under a new name.

## Ownership

```text
lgui-core
    portable effect types, Scene commands, ordering, projection, damage

lgui-render-gdi
    offscreen BGRA capture, CPU blur, clipping, composition, GDI cache

lgui-render-d2d
    intermediate Direct2D images, GaussianBlur effect graph, D2D cache

lgui-render-skia
    SkImageFilter/SkMaskFilter and filtered-layer integration, Skia cache

lgui-platform-win32
    Win32 window/surface integration and generic native pixel interop only

lgui-assets
    image loading, decoding, identity, and lifetime; no blur algorithm
```

Shared conformance fixtures can live outside individual backends. Production blur algorithms and
native effect objects must remain owned by the backend that executes them. A CPU implementation
may be extracted into a renderer-neutral raster utility only if more than one renderer actually
uses the same algorithm and pixel contract; it must not be placed in `lgui-platform-win32` merely
because GDI and Direct2D run on Windows.

## Backend Notes

### GDI

Classic GDI has no native Gaussian-blur filter. The GDI backend can still support all portable blur
semantics by rendering or copying the required region into a 32-bit premultiplied BGRA DIB,
filtering it on the CPU, and compositing it back with the correct clip and alpha.

True backdrop blur should operate on the renderer's current offscreen frame whenever possible.
Reading arbitrary screen pixels is not a reliable substitute and would produce incorrect results
with occlusion or desktop composition. Large sigma values may use downsampling and repeated box
passes, but they must conform to the same sigma and edge contract as the other backends.

### Direct2D

The Direct2D backend should render the effect input to an intermediate image or command list and
feed that image to `CLSID_D2D1GaussianBlur`. The implementation should set standard deviation,
optimization, and border properties deliberately. Hardware acceleration is expected when the
active Direct2D device supports it, but semantic correctness cannot depend on the work running on
the GPU.

Backdrop blur still requires the renderer to make previously drawn content available as an input;
creating a Gaussian effect alone does not establish backdrop semantics.

### Skia

Skia's `SkImageFilter` filters rasterized drawing output. It can therefore process a decoded image,
the output of shapes and text drawn into a saved layer, or another filter's output. Image assets do
not own this capability.

The Skia backend should select the appropriate filter/layer path for each target. The active Skia
surface decides whether work runs through a GPU implementation or the raster pipeline; the public
effect semantics remain identical.

## Current Implementation Mismatch

The current `BackdropBlurStyle` contains `source`, `fit`, and `source_rect`. All three renderers use
that source image and blur its pixels:

- Skia loads the image, installs `image_filters::blur` on a paint, and draws that image.
- Direct2D calls the shared Win32 CPU helper, uploads the resulting BGRA pixels, and draws a bitmap.
- GDI calls the same shared Win32 CPU helper and blits the resulting bitmap.

This behavior is image blur or background-image blur, not backdrop blur. The shared CPU blur and
its caches currently live under `lgui-platform-win32/src/render_support/blur.rs`, even though the
algorithm is renderer behavior. Direct2D also does not currently use its native Gaussian effect.

## Migration Plan

1. Inventory all callers of `BackdropBlurStyle` and determine whether each caller expects a fixed
   source image or the pixels behind the element.
2. Preserve the existing source-image behavior under an accurately named image/effect API so that
   applications can migrate without an accidental visual change.
3. Introduce the portable Gaussian blur parameter contract and normalize sigma, scale projection,
   edge handling, tint order, opacity, and damage expansion.
4. Add true backdrop and filtered-content Scene semantics with explicit ordering and clip bounds.
5. Move the CPU blur implementation and its cache from `lgui-platform-win32` into
   `lgui-render-gdi`.
6. Replace the Direct2D CPU/upload path with a Direct2D effect graph and renderer-owned cache.
7. Update Skia to use the appropriate layer or backdrop input rather than a required static source.
8. Remove the old misleading types and compatibility path after all internal callers migrate.
9. Add cross-backend conformance scenes for image, content, backdrop, and mask blur at multiple
   scale factors, sigma values, edge modes, clips, opacity values, and moving backgrounds.
10. Update `API.md`, `ARCHITECTURE.md`, crate READMEs, and examples, then retire this draft.

## Acceptance Criteria

- No public type named backdrop blur requires an image source.
- Blur ownership does not reside in `lgui-assets` or either platform crate.
- GDI, Direct2D, and Skia render the same Scene effect with comparable bounds and intensity.
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
- The exact visual tolerance and reference backend for cross-renderer conformance tests.
- Whether GDI uses an exact separable Gaussian at small sigma and a downsampled approximation at
  large sigma, or one approximation strategy for all values.
