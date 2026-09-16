use super::*;

pub(super) fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let target = unsafe { BeginPaint(hwnd, &mut paint) };
    render_window(hwnd, target);
    unsafe {
        let _ = EndPaint(hwnd, &paint);
    }
}

pub(super) fn render_window(hwnd: HWND, target: HDC) {
    let mut committed_memory = None;
    let retry = STATE.with(|state| {
        let mut state = state.borrow_mut();
        let Some(state) = state.get_mut(&(hwnd.0 as isize)) else {
            return false;
        };
        if state.rendering_suspended || state.minimized || unsafe { IsIconic(hwnd).as_bool() } {
            state.session.invalidate_all();
            return false;
        }
        #[cfg(feature = "diagnostics")]
        let frame_started = state.diagnostics.as_ref().map(|_| Instant::now());
        let mut client = RECT::default();
        if let Err(source) = unsafe { GetClientRect(hwnd, &mut client) } {
            report_render_error(
                state,
                RenderErrorStage::Prepare,
                "read_client_bounds",
                &source,
            );
            return schedule_render_retry(state);
        }
        let mut window = RECT::default();
        if let Err(source) = unsafe { GetWindowRect(hwnd, &mut window) } {
            report_render_error(
                state,
                RenderErrorStage::Prepare,
                "read_window_bounds",
                &source,
            );
            return schedule_render_retry(state);
        }
        // Shell minimize/restore transitions can expose iconic geometry before IsIconic changes.
        // Reject it before layout or surface allocation can replace the retained full-size frame.
        if should_skip_window_geometry(client, window) {
            state.session.invalidate_all();
            return false;
        }
        let physical = PhysicalSize::new(client.right - client.left, client.bottom - client.top);
        let dpi = DpiContext::for_window(hwnd, state.logical_size);
        let logical = dpi.scale.logical_size(physical);
        let viewport = UiRect::new(0.0, 0.0, logical.width, logical.height);
        #[cfg(feature = "diagnostics")]
        let build_started = state.diagnostics.as_ref().map(|_| Instant::now());
        let commit = state.session.render_view(&state.view, viewport, dpi.scale);
        #[cfg(feature = "diagnostics")]
        let frame_build_duration = build_started.map(|started| started.elapsed());
        let physical_damage = commit
            .damage
            .dirty
            .effective_rects()
            .into_iter()
            .filter_map(|rect| {
                dpi.scale
                    .physical_rect_outward(rect)
                    .intersect(PhysicalRect::new(0, 0, physical.width, physical.height))
            })
            .collect::<Vec<_>>();
        #[cfg(feature = "images-win32")]
        if !state
            .image_reachability_scene
            .as_ref()
            .is_some_and(|previous| previous.shares_command_storage_with(&commit.scene))
        {
            lgui_core::backend::update_image_reachability(
                state.memory_instance,
                &commit.scene.image_requests(),
            );
            state.image_reachability_scene = Some(commit.scene.clone());
        }
        let scene = commit.scene.project_to_physical(dpi.scale);
        let physical_viewport = PhysicalRect::new(0, 0, physical.width, physical.height);
        if state.renderer.is_none() {
            match state.renderer_factory.create(hwnd) {
                Ok(mut renderer) => {
                    renderer.set_memory_budget(state.renderer_budget.load(Ordering::Acquire));
                    state.renderer = Some(renderer);
                }
                Err(source) => {
                    report_render_error(
                        state,
                        RenderErrorStage::Create,
                        "recreate_renderer",
                        &source,
                    );
                    return schedule_render_retry(state);
                }
            }
        }
        #[cfg(feature = "diagnostics")]
        if state.diagnostics.is_some() {
            state.renderer_factory.reset_present_metrics();
        }
        #[cfg(feature = "diagnostics")]
        let draw_started = state.diagnostics.as_ref().map(|_| Instant::now());
        let frame_reason = if state.render_retry_used {
            FrameReason::Recovery
        } else if physical_damage.is_empty() {
            FrameReason::PlatformExposure
        } else {
            FrameReason::SceneChange
        };
        let frame = FrameInfo::new(
            physical_viewport,
            &physical_damage,
            dpi.scale,
            frame_reason,
            commit.damage.dirty.is_full(),
        );
        let mut render_target = Win32RenderTarget::new(hwnd, target);
        #[cfg(feature = "images")]
        let result = {
            let resources = state
                .context
                .try_resource::<lgui_core::assets::RenderResources>()
                .map(|resources| (*resources).clone())
                .unwrap_or_default();
            let Some(renderer) = state.renderer.as_mut() else {
                return false;
            };
            let render = || {
                lgui_core::backend::with_render_resources(&state.context, resources, || {
                    renderer.prepare(&mut render_target, &frame)?;
                    renderer.render(&mut render_target, &scene, &frame)
                })
            };
            render()
        };
        #[cfg(not(feature = "images"))]
        let result = state
            .renderer
            .as_mut()
            .expect("renderer was created before drawing")
            .prepare(&mut render_target, &frame)
            .and_then(|_| {
                state
                    .renderer
                    .as_mut()
                    .expect("renderer was created before drawing")
                    .render(&mut render_target, &scene, &frame)
            });
        #[cfg(feature = "diagnostics")]
        let draw_present_duration = draw_started.map(|started| started.elapsed());
        #[cfg(feature = "diagnostics")]
        let present_metrics = state.diagnostics.as_ref().map(|_| {
            state
                .renderer_factory
                .take_present_metrics(rect_pixels(&physical_damage))
        });
        match result {
            Ok(_stats) => {
                state.render_retry_used = false;
                if lgui_core::backend::memory_begin_frame_budget_check(state.context.memory()) {
                    if let Some(renderer) = state.renderer.as_ref() {
                        *state
                            .renderer_memory
                            .lock()
                            .expect("renderer memory usage poisoned") = renderer.memory_usage();
                    }
                    update_session_memory_usage(state);
                    committed_memory = Some(state.context.memory().clone());
                }
                state.session.runtime().run_effects();
                #[cfg(feature = "diagnostics")]
                if let Some(diagnostics) = state.diagnostics.clone() {
                    state.frame_index = state.frame_index.wrapping_add(1);
                    let layout = state.session.layout_metrics();
                    let components = state.session.component_metrics();
                    let projection = state.session.projection_metrics();
                    #[cfg(feature = "diagnostics-timing")]
                    let timings = state.session.render_timings();
                    diagnostics.record(
                        FrameSample {
                            frame_index: state.frame_index,
                            recorded_at: Instant::now(),
                            backend: state.renderer_factory.name(),
                            renderer: lgui_core::diagnostics::RendererDeviceInfo {
                                api: state.renderer_factory.name().to_owned(),
                                color_format: "BGRA8 premultiplied".to_owned(),
                                present_mode: "Win32 immediate".to_owned(),
                                ..lgui_core::diagnostics::RendererDeviceInfo::default()
                            },
                            mode: if physical_damage.is_empty() {
                                DiagnosticPresentMode::Skipped
                            } else if commit.damage.dirty.is_full() {
                                DiagnosticPresentMode::Full
                            } else {
                                DiagnosticPresentMode::Dirty
                            },
                            frame_build_ms: frame_build_duration.map_or(0.0, duration_ms),
                            diff_ms: 0.0,
                            draw_present_ms: draw_present_duration.map_or(0.0, duration_ms),
                            total_ms: frame_started
                                .map_or(0.0, |started| duration_ms(started.elapsed())),
                            dirty_rect_count: physical_damage.len(),
                            dirty_area_ratio: commit.damage.dirty.area_ratio(),
                            submit_scope: if commit.damage.dirty.is_full() {
                                "full"
                            } else {
                                "dirty"
                            },
                            fallback_reason: commit.damage.dirty.fallback_reason(),
                            primary_reason: commit.damage.reasons.first().map(damage_reason_label),
                            recovery_state: "healthy",
                            recovery_attempt: 0,
                            render: FrameRenderMetrics {
                                #[cfg(feature = "diagnostics-timing")]
                                build_host_tree_ms: timings.retained_snapshot_ms
                                    + timings.declarative_mount_ms
                                    + timings.focus_animation_sync_ms,
                                #[cfg(not(feature = "diagnostics-timing"))]
                                build_host_tree_ms: frame_build_duration.map_or(0.0, duration_ms),
                                #[cfg(feature = "diagnostics-timing")]
                                pending_updates_ms: timings.pending_updates_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                prepare_render_ms: timings.prepare_render_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                retained_snapshot_ms: timings.retained_snapshot_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                declarative_mount_ms: timings.declarative_mount_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                focus_animation_sync_ms: timings.focus_animation_sync_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                focus_sync_ms: timings.focus_sync_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                focus_rebuild_ms: timings.focus_rebuild_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                animation_target_sync_ms: timings.animation_target_sync_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                animation_rebuild_ms: timings.animation_rebuild_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                runtime_reconcile_ms: timings.runtime_reconcile_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                layout_ms: timings.layout_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_commit_ms: timings.host_commit_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_change_scan_ms: commit.timings.change_scan_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_node_patch_ms: commit.timings.node_patch_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_scene_reconcile_ms: commit.timings.scene_reconcile_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_scene_snapshot_ms: commit.timings.scene_snapshot_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_damage_ms: commit.timings.damage_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_finalize_ms: commit.timings.finalize_ms,
                                #[cfg(feature = "diagnostics-timing")]
                                host_unattributed_ms: (timings.host_commit_ms
                                    - commit.timings.change_scan_ms
                                    - commit.timings.node_patch_ms
                                    - commit.timings.scene_reconcile_ms
                                    - commit.timings.scene_snapshot_ms
                                    - commit.timings.damage_ms
                                    - commit.timings.finalize_ms)
                                    .max(0.0),
                                #[cfg(feature = "diagnostics-timing")]
                                render_total_ms: timings.total_ms,
                                #[cfg(not(feature = "diagnostics-timing"))]
                                render_total_ms: frame_build_duration.map_or(0.0, duration_ms),
                                node_count: state.session.tree().nodes().len(),
                                command_count: scene.commands().len(),
                                component_executed: components.executed,
                                component_dirty: components.dirty,
                                projection_visited_nodes: projection.visited_nodes,
                                projection_reused_component_roots: projection
                                    .reused_component_roots,
                                #[cfg(feature = "diagnostics-timing")]
                                animation_sync_nodes: timings.animation_sync_nodes,
                                #[cfg(feature = "diagnostics-timing")]
                                focus_sync_needed: timings.focus_sync_needed,
                                layout_visited_nodes: layout.visited_nodes,
                                layout_laid_out_nodes: layout.laid_out_nodes,
                                layout_reused_nodes: layout.reused_nodes,
                                host_visited_nodes: commit.metrics.visited_host_nodes,
                                scene_compiled_nodes: commit.metrics.compiled_scene_nodes,
                                host_mutations: commit.metrics.host_mutations,
                                scene_mutations: commit.metrics.scene_mutations,
                                reused_scene_nodes: commit.metrics.reused_scene_nodes,
                                ..FrameRenderMetrics::default()
                            },
                            present: present_metrics.unwrap_or_default(),
                        },
                        state.session.tree(),
                        viewport,
                    );
                }
                false
            }
            Err(error) => {
                let source = error.source_error();
                lgui_core::backend::report_render_error(
                    &state.context,
                    lgui_core::backend::render_error(
                        state.id.clone(),
                        state.renderer_factory.name(),
                        error.stage(),
                        error.operation(),
                        source.code().0,
                        source.to_string(),
                    ),
                );
                schedule_render_retry(state)
            }
        }
    });
    if let Some(memory) = committed_memory {
        lgui_core::backend::memory_finish_frame_budget_check(&memory);
    }
    if retry {
        unsafe {
            let _ = InvalidateRect(Some(hwnd), None, false);
        }
    }
}

pub(super) fn should_skip_window_geometry(client: RECT, window: RECT) -> bool {
    client.right - client.left <= 1
        || client.bottom - client.top <= 1
        || window.left <= -30000
        || window.top <= -30000
}

#[cfg(feature = "diagnostics")]
pub(super) fn rect_pixels(rects: &[PhysicalRect]) -> u64 {
    rects.iter().fold(0_u64, |total, rect| {
        total.saturating_add(
            (rect.width().max(0) as u64).saturating_mul(rect.height().max(0) as u64),
        )
    })
}

#[cfg(feature = "diagnostics")]
pub(super) fn damage_reason_label(reason: &DamageReason) -> &'static str {
    match reason {
        DamageReason::FirstCommit => "first-commit",
        DamageReason::Explicit => "explicit",
        DamageReason::Insert => "node-added",
        DamageReason::Remove => "node-removed",
        DamageReason::Layout => "layout",
        DamageReason::Paint => "paint",
        DamageReason::Interaction => "interaction",
        DamageReason::Structure => "structure",
        DamageReason::Clean => "clean",
    }
}

pub(super) fn report_render_error(
    state: &WindowState,
    stage: RenderErrorStage,
    operation: &'static str,
    source: &Error,
) {
    lgui_core::backend::report_render_error(
        &state.context,
        lgui_core::backend::render_error(
            state.id.clone(),
            state.renderer_factory.name(),
            stage,
            operation,
            source.code().0,
            source.to_string(),
        ),
    );
}

pub(super) fn schedule_render_retry(state: &mut WindowState) -> bool {
    state.session.invalidate_all();
    if state.render_retry_used {
        return false;
    }
    state.render_retry_used = true;
    true
}
