# LGUI capability implementation

Scope: `LGUI_REQUIREMENTS.md` in the parent Loom repository, with the user's
2026-11-02 update: complete LGUI independently before integrating Loom.
This repository contains the library implementation, standalone examples and
behavioral tests. Application integration and language-extension evidence are
recorded separately in Loom's `docs/architecture/extension-language-migration.md`.

The following checkboxes mean implemented and independently tested in LGUI. They
do not by themselves mark Loom's PG gate complete. Loom now has the required
native use sites and API adaptations; their acceptance remains in that repository.

| Done | Requirement | Implementation | Executable evidence / standalone use |
| --- | --- | --- | --- |
| [x] | LY0 | Shared Dimension/SizeConstraints, text/container measurement, min/max/aspect/percentage, Clip/Ellipsis/Collapse | `layout/sizing.rs`, `sizing_runtime_test.rs`; measured grapheme ellipsis and collapse tests; gallery Grid/input |
| [x] | LY1/LY4 | Flex grow/shrink/basis, padding/gap, justification, alignment and wrapping; Stack removed | `layout/containers_test.rs` allocation, minimum/maximum and wrap tests; gallery footer and Split; counter/README migrated |
| [x] | LY2 | Ordered edge Dock and remaining Fill | `dock_reserves_edges_then_fills_remaining`; gallery window structure |
| [x] | LY3 | Fixed/content/fraction Grid tracks, gaps, cell spans and automatic placement | `grid_fixed_auto_fraction_tracks_and_spans`, `grid_auto_placement_skips_spanning_cells`; gallery input column |
| [x] | PS1 | Separate Position::Absolute outside sibling layout | `position_test.rs` flow/absolute test |
| [x] | PS2 | Aligned/offset Layer plus stable z order for painting and hit testing | `absolute_and_layer_are_out_of_flow_and_have_predictable_z_order`; gallery floating content |
| [x] | PS3 | Public-ID Anchor, flip, viewport origin clamp, dependency/error handling | Anchor flip/target movement/missing/cycle tests; gallery floating status |
| [x] | PS4 | Sticky preserves flow and follows nearest scroll clip | `sticky_headers_follow_scroll_edges_and_return_to_their_flow_position` |
| [x] | CP1 | Portable input/multiline, ring, scrollbar and resize handle | `widgets/text_input.rs` editing tests; `capabilities_test.rs` IME/password/read-only/selection/semantic value/translated hit and caret; `controls.rs` ring/drag tests; gallery |
| [x] | CP2 | Flex Split with clamped dragging, minimums and ratio writeback | `split_drag_updates_ratio_and_preserves_both_minimums`; gallery |
| [x] | CP3 | Uniform/variable virtual lists, keyed expandable trees, tabs | Million-row construction count test, variable prefix/range tests, tree/tab keyboard and selection tests; gallery |
| [x] | CP4 | Theme-backed badge and accessible owned-key icon button | Widget runtime hit/semantics coverage and dynamic scene key tests; gallery footer |
| [x] | CP5 | Outside-pointer and Escape close intent; reuse focus scopes/restoration | `outside_pointer_is_direct_and_escape_is_scoped_without_removing_nodes`; existing focus scope trap/restore test; gallery application-owned floating content |
| [x] | IN1/IN2 | Optional window-unique public IDs; readable named semantic projection with no private UiIds | `view/public.rs` private-node projection, duplicate rejection and ancestor routing tests; gallery IDs |
| [x] | IN3 | Owned Arc string keys through node, scene, icon registry and Skia | `dynamic_keys_outlive_their_callers_and_reach_the_scene`; renderer compilation/tests; gallery owned String key |
| [x] | IN4 | Colors, spacing, typography, radius, motion defaults; explicit styles/colors allowed | Theme transition override assertion, themed controls; gallery custom colors |
| [x] | IN5 | Public-ID click/input/selection/change/visibility routing | Public/raw event mapping, input committed event callback and description show/hide/removal tests |
| [x] | IN6 | Runtime descriptions diff into the existing HostTree; keyed identity, atomic validation and reclamation | Description unchanged/shared-storage/reorder/update/delete/atomic rejection tests |
| [x] | IN7 | Presence combines retained opacity/translation and existing frame scheduling | Transition reversal/zero-duration tests and runtime exit interaction/collapse test; gallery |

Existing implementation: spacing and typography tokens, focus scopes, scroll
views, selection regions, retained component/host reconciliation. Reuse these
mechanisms rather than create a second runtime.

Source paths in the table are relative to `crates/lgui-core/src/core` or
`crates/lgui-widgets/src`. The complete public contract and API migration notes
are in [API.md](API.md). Run the native gallery with:

```powershell
cargo run --example capabilities --features renderer-skia-software
```

Validation on Windows, Rust 1.98.0, using the shared build cache only (no Loom
integration): core and widgets with all features pass **226 + 45 = 271** unit
tests. The portable core's original baseline was 171 tests. Ordinary/default
feature coverage was also run during implementation. All-target/all-feature
compilation includes the gallery, Skia and the native accessibility adapter.

The final conditional workspace regression passes **389 tests**, with the two
named baseline failures below filtered out. The router tests require an 8 MiB
test-thread stack in this Windows debug build:

```powershell
$env:RUST_MIN_STACK = '8388608'
cargo test --workspace --all-features --offline -- --skip portable_skia_contains_no_window_system_adapter_code --skip persistent_image_revalidation_reuses_cached_bytes_after_304
cargo clippy --workspace --all-targets --all-features --offline
```

Clippy completes successfully in ordinary warning mode; all diagnostics in newly
added files were resolved. Existing warnings in unchanged code remain, so this is
not a claim that `-D warnings` passes. `git diff --check` also passes.

Broader pre-existing regression findings are recorded separately from the new
capabilities:

- `lgui-core/tests/architecture_test.rs` rejects `Win32::` in Skia
  `backend/rasterization.rs`; the same direct platform call exists at `bb7489f`.
- The all-feature assets test
  `persistent_image_revalidation_reuses_cached_bytes_after_304` fails its refreshed
  cache-entry assertion. The same failure was reproduced independently from the
  unmodified `bb7489f` snapshot with the same dependency lock.
- The router test
  `nested_outlet_preserves_parent_layout_lifecycle_across_child_navigation`
  overflows the default Windows debug test-thread stack. This was also reproduced
  in the unmodified snapshot. With `RUST_MIN_STACK=8388608`, all 21 router tests
  pass in the implementation checkout. No global environment setting was changed.
- The backdrop example has existing Rust 2024 unsafe-operation warnings. Strict
  Clippy already had baseline warnings; these are not described as a clean strict
  lint run.

The gallery has compile coverage and runtime behavior is covered by the above
headless tests. Interactive native visual QA and Linux/macOS runtime testing are
not claimed. This library record does not claim completion of the separate
language-client migration or removal of the host LSP.
