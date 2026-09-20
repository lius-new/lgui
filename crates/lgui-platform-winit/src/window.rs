use super::*;

pub(super) struct WinitWindow {
    pub(super) id: WindowId,
    pub(super) options: WindowOptions,
    pub(super) view: AppView,
    pub(super) context: ApplicationContext,
    pub(super) window: Arc<Window>,
    pub(super) renderer: WinitSkiaRenderer,
    pub(super) soft_context: Arc<SoftContext<OwnedDisplayHandle>>,
    pub(super) preference: GraphicsPreference,
    pub(super) session: UiSession,
    pub(super) scale: UiScale,
    pub(super) cursor: Option<Point>,
    pub(super) active_cursor: Option<WinitCursorIcon>,
    pub(super) modifiers: ModifiersState,
    pub(super) visible: bool,
    pub(super) owner_suppressed: bool,
    pub(super) occluded: bool,
    pub(super) ime_allowed: bool,
    pub(super) last_frame: Instant,
    pub(super) next_frame: Option<Instant>,
    pub(super) full_redraw: bool,
    pub(super) recovery: RendererRecoveryState,
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    pub(super) memory_instance: lgui_core::memory::DomainInstanceId,
    pub(super) memory_budget: usize,
    #[cfg(feature = "diagnostics")]
    pub(super) diagnostics: Option<Arc<DiagnosticsRegistration>>,
    #[cfg(feature = "diagnostics")]
    pub(super) frame_index: u64,
    #[cfg(feature = "accessibility")]
    pub(super) accessibility: super::winit_accessibility::AccessibilityState,
}

pub(super) fn prepare_auxiliary_window(
    context: &ApplicationContext,
    main_id: &WindowId,
    mut options: WindowOptions,
    view: AppView,
) -> (WindowOptions, AppView) {
    if options.owner.is_none() && options.id != *main_id {
        options.owner = Some(main_id.clone());
    }
    (options, application_root_view(context.clone(), view))
}

impl WinitWindow {
    pub(super) fn screen_cursor_position(&self) -> Option<PhysicalPosition<i32>> {
        let cursor = self.cursor?;
        let origin = self.window.outer_position().ok()?;
        let point = self.scale.physical_point(cursor);
        Some(PhysicalPosition::new(
            origin.x + point.x,
            origin.y + point.y,
        ))
    }

    pub(super) fn handle_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.render(),
            WindowEvent::Resized(_) => {
                self.full_redraw = true;
                self.session.invalidate_all();
                self.window.request_redraw();
                self.sync_mode_after_resize();
            }
            WindowEvent::ScaleFactorChanged { .. } => {
                self.update_scale();
                self.dispatch_input(InputEvent::Platform(
                    lgui_core::core::PlatformEvent::ScaleFactorChanged(self.window.scale_factor()),
                ));
            }
            WindowEvent::Focused(focused) => {
                self.context.emit(lgui_core::window::WindowFocusChanged {
                    window_id: self.id.clone(),
                    focused,
                });
                self.dispatch_input(InputEvent::Platform(
                    lgui_core::core::PlatformEvent::Focused(focused),
                ));
                if !focused && self.options.hide_on_deactivate {
                    self.set_desired_visibility(false);
                }
            }
            WindowEvent::ThemeChanged(theme) => self.dispatch_input(InputEvent::Platform(
                lgui_core::core::PlatformEvent::ThemeChanged(match theme {
                    winit::window::Theme::Light => lgui_core::core::PlatformTheme::Light,
                    winit::window::Theme::Dark => lgui_core::core::PlatformTheme::Dark,
                }),
            )),
            WindowEvent::Occluded(occluded) => self.occluded = occluded,
            WindowEvent::CursorMoved { position, .. } => {
                let point = self.scale.logical_point(PhysicalPoint::new(
                    position.x.round() as i32,
                    position.y.round() as i32,
                ));
                self.cursor = Some(point);
                self.sync_cursor(point);
                self.dispatch_input(InputEvent::PointerMove(PointerData::mouse(point)));
            }
            WindowEvent::CursorEntered { .. } => {
                if let Some(point) = self.cursor {
                    self.dispatch_input(InputEvent::PointerEnter(PointerData::mouse(point)));
                }
            }
            WindowEvent::CursorLeft { .. } => {
                let point = self.cursor.unwrap_or_default();
                self.dispatch_input(InputEvent::PointerLeave(PointerData::mouse(point)));
                self.cursor = None;
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let pointer = PointerData::mouse(self.cursor.unwrap_or_default());
                let button = pointer_button(button);
                if state == ElementState::Pressed
                    && button == PointerButton::Left
                    && !self.options.native_titlebar
                {
                    let size = self.window.inner_size();
                    let physical = self.scale.physical_point(pointer.point);
                    if self.options.resizable {
                        if let Some(direction) = resize_direction(
                            physical,
                            WinitPhysicalSize::new(size.width, size.height),
                            (6.0 * self.scale.factor()).ceil().max(1.0) as i32,
                        ) {
                            let _ = self.window.drag_resize_window(direction);
                        } else if self
                            .session
                            .tree()
                            .hit_test(pointer.point)
                            .is_some_and(|hit| {
                                hit.interaction
                                    == lgui_core::core::InteractionRole::WindowDragRegion
                            })
                        {
                            let _ = self.window.drag_window();
                        }
                    } else if self
                        .session
                        .tree()
                        .hit_test(pointer.point)
                        .is_some_and(|hit| {
                            hit.interaction == lgui_core::core::InteractionRole::WindowDragRegion
                        })
                    {
                        let _ = self.window.drag_window();
                    }
                }
                self.dispatch_input(match state {
                    ElementState::Pressed => InputEvent::PointerDown { pointer, button },
                    ElementState::Released => InputEvent::PointerUp { pointer, button },
                });
            }
            WindowEvent::MouseWheel { delta, phase, .. } => {
                let point = self.cursor.unwrap_or_default();
                let phase = touch_phase(phase);
                let delta = match delta {
                    MouseScrollDelta::LineDelta(x, y) => WheelDelta {
                        x,
                        y,
                        unit: lgui_core::core::WheelUnit::Lines,
                        phase,
                    },
                    MouseScrollDelta::PixelDelta(position) => WheelDelta {
                        x: position.x as f32 / self.scale.factor(),
                        y: position.y as f32 / self.scale.factor(),
                        unit: lgui_core::core::WheelUnit::Pixels,
                        phase,
                    },
                };
                self.dispatch_input(InputEvent::Wheel { point, delta });
            }
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } => {
                self.dispatch_input(InputEvent::Keyboard(keyboard_event(event, self.modifiers)));
            }
            WindowEvent::Ime(event) => self.dispatch_input(InputEvent::Ime(match event {
                Ime::Enabled => ImeEvent::Enabled,
                Ime::Preedit(text, cursor) => ImeEvent::Preedit {
                    text,
                    cursor: cursor.map(|(start, end)| start..end),
                },
                Ime::Commit(text) => ImeEvent::Commit(text),
                Ime::Disabled => ImeEvent::Disabled,
            })),
            WindowEvent::Touch(touch) => {
                let point = self.scale.logical_point(PhysicalPoint::new(
                    touch.location.x.round() as i32,
                    touch.location.y.round() as i32,
                ));
                self.dispatch_input(InputEvent::Touch {
                    pointer: PointerData {
                        id: PointerId(touch.id),
                        kind: PointerKind::Touch,
                        point,
                        pressure: touch.force.map(|force| {
                            (force.normalized().clamp(0.0, 1.0) * u16::MAX as f64).round() as u16
                        }),
                        primary: touch.id == 0,
                    },
                    phase: touch_phase(touch.phase),
                });
            }
            WindowEvent::HoveredFile(path) => self.dispatch_input(InputEvent::Platform(
                lgui_core::core::PlatformEvent::FileHovered(path),
            )),
            WindowEvent::DroppedFile(path) => self.dispatch_input(InputEvent::Platform(
                lgui_core::core::PlatformEvent::FileDropped(path),
            )),
            WindowEvent::HoveredFileCancelled => self.dispatch_input(InputEvent::Platform(
                lgui_core::core::PlatformEvent::FileHoverCancelled,
            )),
            _ => {}
        }
    }

    /// Detects when the OS has pulled the window out of fullscreen on its own
    /// (dragging an edge to shrink it, or dragging the titlebar to move it,
    /// which Windows turns into a restore). winit 0.30 does not track this, so
    /// lgui's `options.mode` would otherwise stay `Fullscreen`: corners stay
    /// square and the system caption leaks back in. Re-apply a windowed state
    /// so the window matches what the OS is actually showing.
    fn sync_mode_after_resize(&mut self) {
        if self.options.mode != WindowMode::Fullscreen {
            return;
        }
        let Some(monitor) = self.window.current_monitor() else {
            return;
        };
        let monitor_size = monitor.size();
        let inner = self.window.inner_size();
        if inner.width >= monitor_size.width && inner.height >= monitor_size.height {
            return;
        }
        // The OS restored the window; keep the user's current size/position.
        let outer = self.window.outer_position().ok();
        self.options.mode = WindowMode::Windowed;
        self.window.set_fullscreen(None);
        #[cfg(target_os = "windows")]
        super::winit_windows::set_corner_radius(
            &self.window,
            self.options.corner_radius,
            WindowMode::Windowed,
        );
        if let Some(position) = outer {
            let _ = self.window.set_outer_position(position);
        }
        let _ = self.window.request_inner_size(inner);
    }

    pub(super) fn dispatch_input(&mut self, input: InputEvent) {
        let output = self.session.handle_input(input);
        let mut context_frame = false;
        let output = dispatch_runtime_output(
            output,
            &self.context,
            &self.id,
            |action| self.session.handle_default_action(action),
            |context| context_frame |= context.flags().needs_frame,
        );
        if context_frame {
            self.session.invalidate_all();
            self.full_redraw = true;
        }
        if let Some(bounds) = output.dirty_bounds {
            self.session.invalidations_mut().invalidate_rect(bounds);
        }
        self.sync_ime();
        self.apply_pending_updates();
        if output.animation_changed {
            self.schedule_next_frame();
        }
    }

    /// Updates the native cursor icon for the current pointer position.
    ///
    /// For frameless, resizable windows the OS no longer provides edge-resize
    /// cursors, so we resolve the resize direction first, then fall back to the
    /// cursor requested by the frontmost node under the pointer.
    pub(super) fn sync_cursor(&mut self, point: Point) {
        let icon = self.resolve_cursor(point);
        if self.active_cursor != Some(icon) {
            self.window.set_cursor(icon);
            self.active_cursor = Some(icon);
        }
    }

    fn resolve_cursor(&self, point: Point) -> WinitCursorIcon {
        if !self.options.native_titlebar && self.options.resizable {
            let size = self.window.inner_size();
            let physical = self.scale.physical_point(point);
            if let Some(direction) = resize_direction(
                physical,
                WinitPhysicalSize::new(size.width, size.height),
                (6.0 * self.scale.factor()).ceil().max(1.0) as i32,
            ) {
                return edge_resize_cursor(direction);
            }
        }
        mapped_cursor(
            self.session
                .tree()
                .cursor_at(point)
                .unwrap_or(CursorIcon::Default),
        )
    }

    pub(super) fn apply_pending_updates(&mut self) {
        let updates = self.session.apply_pending_updates();
        let has_dirty_ids = !updates.dirty_ids.is_empty();
        if updates.focus_changed {
            self.session.invalidate_all();
            self.full_redraw = true;
        } else if !updates.dirty_ids.is_empty() {
            if let Some(bounds) = self.session.tree().paint_bounds(updates.dirty_ids) {
                self.session.invalidations_mut().invalidate_rect(bounds);
            } else {
                self.session.invalidate_all();
                self.full_redraw = true;
            }
        }
        if updates.frame_requested {
            self.schedule_next_frame();
        }
        if self.full_redraw
            || has_dirty_ids
            || (updates.frame_requested && self.next_frame.is_none())
        {
            self.window.request_redraw();
        }
    }

    pub(super) fn sync_ime(&mut self) {
        let focused = self
            .session
            .runtime()
            .interaction_state()
            .focused
            .as_ref()
            .and_then(|id| self.session.tree().node(id))
            .and_then(|node| {
                let role = node.semantics.as_ref()?.role;
                matches!(
                    role,
                    lgui_core::core::SemanticRole::TextInput
                        | lgui_core::core::SemanticRole::PasswordInput
                        | lgui_core::core::SemanticRole::SearchInput
                )
                .then_some((node.ime_cursor_rect.unwrap_or(node.layout_rect), role))
            });
        let allowed = focused.is_some();
        if allowed != self.ime_allowed {
            self.window.set_ime_allowed(allowed);
            self.ime_allowed = allowed;
        }
        let Some((rect, role)) = focused else {
            return;
        };
        self.window.set_ime_purpose(
            (role == lgui_core::core::SemanticRole::PasswordInput)
                .then_some(ImePurpose::Password)
                .unwrap_or(ImePurpose::Normal),
        );
        let point = self
            .scale
            .physical_point(Point::new(rect.left, rect.bottom + 2.0));
        let height = self.scale.physical_length(rect.height().max(1.0));
        self.window.set_ime_cursor_area(
            PhysicalPosition::new(point.x, point.y),
            WinitPhysicalSize::new(1_u32, height.max(1) as u32),
        );
    }

    pub(super) fn advance_animation(&mut self, now: Instant) {
        self.next_frame = None;
        let elapsed = now.saturating_duration_since(self.last_frame);
        let output = self.session.advance(elapsed.as_secs_f32() * 1000.0);
        self.last_frame = now;
        if let Some(bounds) = output.dirty_bounds {
            self.session.invalidations_mut().invalidate_rect(bounds);
        }
        if output.animation_changed {
            self.window.request_redraw();
        }
        self.schedule_next_frame();
    }

    pub(super) fn schedule_next_frame(&mut self) {
        self.next_frame = next_frame_deadline(
            self.next_frame,
            Instant::now(),
            self.session.runtime().frame_interval_ms(),
        );
    }

    pub(super) fn update_scale(&mut self) {
        let monitor = self.window.current_monitor();
        self.scale = scale_for_monitor(monitor.as_ref(), &self.options);
        self.session.invalidate_all();
        self.full_redraw = true;
        self.window.request_redraw();
    }

    pub(super) fn render(&mut self) {
        if !self.visible
            || self.owner_suppressed
            || self.occluded
            || matches!(self.recovery, RendererRecoveryState::Failed { .. })
        {
            return;
        }
        let size = self.window.inner_size();
        if size.width == 0 || size.height == 0 {
            return;
        }
        let physical = PhysicalSize::new(size.width as i32, size.height as i32);
        let logical = self.scale.logical_size(physical);
        let viewport = UiRect::new(0.0, 0.0, logical.width, logical.height);
        #[cfg(feature = "images")]
        let _image_cache = self
            .context
            .try_resource::<lgui_assets::ImageCacheHandle>()
            .map(|cache| lgui_assets::backend::install_image_cache((*cache).clone()));
        #[cfg(feature = "diagnostics")]
        let frame_started = Instant::now();
        let commit = self.session.render_view(&self.view, viewport, self.scale);
        #[cfg(feature = "diagnostics")]
        let frame_build_ms = frame_started.elapsed().as_secs_f32() * 1_000.0;
        #[cfg(feature = "accessibility")]
        self.accessibility.publish(
            &commit.semantics,
            self.session.tree(),
            self.session.runtime().interaction_state().focused,
            self.scale,
        );
        let physical_viewport = PhysicalRect::new(0, 0, physical.width, physical.height);
        let damage = if self.full_redraw {
            vec![physical_viewport]
        } else {
            commit
                .damage
                .dirty
                .effective_rects()
                .into_iter()
                .filter_map(|rect| {
                    self.scale
                        .physical_rect_outward(rect)
                        .intersect(physical_viewport)
                })
                .collect::<Vec<_>>()
        };
        #[cfg(feature = "images")]
        lgui_assets::backend::update_image_reachability(
            self.memory_instance,
            &commit.scene.image_requests(),
        );
        if damage.is_empty() && !self.full_redraw {
            self.schedule_next_frame();
            return;
        }
        let scene = commit.scene.project_to_physical(self.scale);
        let frame = FrameInfo::new(
            physical_viewport,
            &damage,
            self.scale,
            if self.full_redraw {
                FrameReason::Resize
            } else {
                FrameReason::SceneChange
            },
            self.full_redraw || commit.damage.dirty.is_full(),
        );
        #[cfg(feature = "images")]
        let resources = self
            .context
            .try_resource::<lgui_assets::RenderResources>()
            .map(|resources| (*resources).clone())
            .unwrap_or_default();
        #[cfg(feature = "diagnostics")]
        let draw_started = Instant::now();
        #[cfg(feature = "images")]
        let result = lgui_assets::backend::with_render_resources(&self.context, resources, || {
            self.renderer.draw_and_present(&scene, &frame, &damage)
        });
        #[cfg(not(feature = "images"))]
        let result = self.renderer.draw_and_present(&scene, &frame, &damage);
        #[cfg(feature = "diagnostics")]
        let draw_present_ms = draw_started.elapsed().as_secs_f32() * 1_000.0;
        let frame_timings = match result {
            Ok(timings) => timings,
            Err(error) => {
                let failed_stage = error.stage;
                let failed_operation = error.operation;
                let failed_message = error.message.clone();
                lgui_core::backend::report_render_error(
                    &self.context,
                    lgui_core::backend::render_error(
                        self.id.clone(),
                        self.renderer.name(),
                        error.stage,
                        error.operation,
                        -1,
                        error.message,
                    ),
                );
                self.recover_renderer(failed_stage, failed_operation, &failed_message);
                return;
            }
        };
        #[cfg(not(feature = "diagnostics"))]
        let _ = frame_timings;
        self.recovery = match self.recovery {
            RendererRecoveryState::Fallback { reason, .. } => RendererRecoveryState::Fallback {
                reason,
                attempts: 0,
            },
            _ => RendererRecoveryState::Healthy,
        };
        #[cfg(feature = "diagnostics")]
        if let Some(diagnostics) = self.diagnostics.clone() {
            self.frame_index = self.frame_index.wrapping_add(1);
            let viewport_pixels = (physical.width.max(1) as u64) * (physical.height.max(1) as u64);
            let dirty_pixels = damage.iter().fold(0_u64, |total, rect| {
                total.saturating_add(rect.width().max(0) as u64 * rect.height().max(0) as u64)
            });
            diagnostics.record(
                FrameSample {
                    frame_index: self.frame_index,
                    recorded_at: Instant::now(),
                    backend: self.renderer.name(),
                    renderer: self.renderer.device_info(),
                    mode: if frame.is_full_redraw() {
                        DiagnosticPresentMode::Full
                    } else {
                        DiagnosticPresentMode::Dirty
                    },
                    frame_build_ms,
                    diff_ms: 0.0,
                    draw_present_ms,
                    total_ms: frame_started.elapsed().as_secs_f32() * 1_000.0,
                    dirty_rect_count: damage.len(),
                    dirty_area_ratio: dirty_pixels as f32 / viewport_pixels as f32,
                    submit_scope: if frame.is_full_redraw() {
                        "full"
                    } else {
                        "dirty"
                    },
                    fallback_reason: self.renderer.fallback_reason(),
                    primary_reason: None,
                    recovery_state: self.recovery.label(),
                    recovery_attempt: self.recovery.attempt(),
                    render: FrameRenderMetrics {
                        build_host_tree_ms: frame_build_ms,
                        render_total_ms: frame_build_ms,
                        node_count: self.session.tree().nodes().len(),
                        command_count: scene.commands().len(),
                        host_visited_nodes: commit.metrics.visited_host_nodes,
                        scene_compiled_nodes: commit.metrics.compiled_scene_nodes,
                        host_mutations: commit.metrics.host_mutations,
                        scene_mutations: commit.metrics.scene_mutations,
                        reused_scene_nodes: commit.metrics.reused_scene_nodes,
                        ..FrameRenderMetrics::default()
                    },
                    present: {
                        let cache = self.renderer.cache_stats();
                        FramePresentMetrics {
                            acquire_ms: frame_timings.acquire_ms,
                            draw_commands_ms: frame_timings.draw_ms,
                            flush_ms: frame_timings.flush_ms,
                            submit_ms: frame_timings.flush_ms,
                            present_ms: frame_timings.present_ms,
                            submitted_pixels: dirty_pixels,
                            fallback_count: usize::from(self.renderer.fallback_reason().is_some()),
                            cache_budget_bytes: cache.budget_bytes,
                            cache_resident_bytes: cache.resident_bytes,
                            cache_entries: cache.entries,
                            cache_hits: cache.hits,
                            cache_misses: cache.misses,
                            cache_evictions: cache.evictions,
                            text_cache_resident_bytes: cache.text_resident_bytes,
                            text_cache_entries: cache.text_entries,
                            text_cache_hits: cache.text_hits,
                            text_cache_misses: cache.text_misses,
                            text_cache_evictions: cache.text_evictions,
                            largest_cache_entry_bytes: cache.largest_entry_bytes,
                            largest_text_cache_entry_bytes: cache.largest_text_entry_bytes,
                            ..FramePresentMetrics::default()
                        }
                    },
                },
                self.session.tree(),
                viewport,
            );
        }
        self.full_redraw = false;
        self.sync_ime();
        self.session.runtime().run_effects();
        self.schedule_next_frame();
    }

    pub(super) fn recover_renderer(
        &mut self,
        stage: lgui_render_api::RenderErrorStage,
        operation: &'static str,
        message: &str,
    ) {
        const MAX_RECOVERY_ATTEMPTS: u8 = 3;
        let attempt = self.recovery.attempt().saturating_add(1);
        let was_fallback = matches!(self.recovery, RendererRecoveryState::Fallback { .. });
        let gpu_failure = match self.recovery {
            RendererRecoveryState::Recovering { gpu_failure, .. } => {
                gpu_failure || self.renderer.is_gpu()
            }
            RendererRecoveryState::Fallback { .. } => true,
            _ => self.renderer.is_gpu(),
        };
        if attempt > MAX_RECOVERY_ATTEMPTS {
            self.recovery = RendererRecoveryState::Failed {
                attempts: MAX_RECOVERY_ATTEMPTS,
            };
            return;
        }

        self.recovery = RendererRecoveryState::Recovering {
            attempt,
            gpu_failure,
        };
        self.renderer.trim(MemoryPressure::Critical);
        let (recovery_preference, fallback_to_software) =
            recovery_preference(self.preference, was_fallback, gpu_failure, attempt);

        let previous = std::mem::replace(&mut self.renderer, WinitSkiaRenderer::Unavailable);
        drop(previous);
        match create_renderer(
            recovery_preference,
            &self.soft_context,
            Arc::clone(&self.window),
            self.options.transparent,
            self.memory_budget,
        ) {
            Ok(mut renderer) => {
                if fallback_to_software {
                    renderer.set_fallback_reason("gpu-runtime-failed");
                }
                let fallback_reason = renderer.fallback_reason();
                self.renderer = renderer;
                if let Some(reason) = fallback_reason {
                    self.recovery = RendererRecoveryState::Fallback {
                        reason,
                        attempts: attempt,
                    };
                }
                self.session.invalidate_all();
                self.full_redraw = true;
                self.window.request_redraw();
            }
            Err(recovery_error) => {
                lgui_core::backend::report_render_error(
                    &self.context,
                    lgui_core::backend::render_error(
                        self.id.clone(),
                        "skia-recovery",
                        lgui_render_api::RenderErrorStage::Create,
                        "recreate_renderer",
                        -1,
                        format!(
                            "attempt {attempt} after {stage:?}/{operation}: {message}; recreation failed: {recovery_error}"
                        ),
                    ),
                );
                if attempt == MAX_RECOVERY_ATTEMPTS {
                    self.recovery = RendererRecoveryState::Failed { attempts: attempt };
                } else {
                    self.window.request_redraw();
                }
            }
        }
    }

    pub(super) fn set_desired_visibility(&mut self, visible: bool) {
        self.visible = visible;
        self.apply_visibility();
    }

    pub(super) fn set_owner_suppressed(&mut self, suppressed: bool) {
        self.owner_suppressed = suppressed;
        self.apply_visibility();
    }

    fn apply_visibility(&mut self) {
        let effective = self.visible && !self.owner_suppressed;
        self.window.set_visible(effective);
        if effective {
            self.full_redraw = true;
            self.session.invalidate_all();
            self.window.request_redraw();
        } else {
            self.suspend_rendering(false);
        }
    }

    pub(super) fn suspend_rendering(&mut self, force: bool) {
        if force || self.options.background_memory_optimization {
            #[cfg(feature = "images")]
            lgui_assets::backend::update_image_reachability(self.memory_instance, &[]);
            self.renderer.trim(MemoryPressure::Critical);
            lgui_core::backend::session_suspend_rendering(&mut self.session);
        }
    }

}

impl Drop for WinitWindow {
    fn drop(&mut self) {
        #[cfg(feature = "images")]
        lgui_assets::backend::update_image_reachability(self.memory_instance, &[]);
    }
}

pub(super) fn initial_scale(event_loop: &ActiveEventLoop, options: &WindowOptions) -> UiScale {
    scale_for_monitor(event_loop.primary_monitor().as_ref(), options)
}

pub(super) fn scale_for_monitor(
    monitor: Option<&winit::monitor::MonitorHandle>,
    options: &WindowOptions,
) -> UiScale {
    let Some(monitor) = monitor else {
        return UiScale::ONE;
    };
    let position = monitor.position();
    let size = monitor.size();
    let work = WorkArea {
        rect: PhysicalRect::new(
            position.x,
            position.y,
            position.x + size.width as i32,
            position.y + size.height as i32,
        ),
    };
    ScaleContext::resolve(
        (monitor.scale_factor() * BASE_DPI as f64).round() as u32,
        work,
        options.scale_reference_size.unwrap_or(options.size),
        options.scale_preference,
    )
    .scale
}

pub(super) fn position_window(
    window: &Window,
    options: &WindowOptions,
    owner: Option<&Window>,
    cursor: Option<PhysicalPosition<i32>>,
) {
    if let WindowPosition::AdjacentToOwner { gap } = options.position {
        if let Some(owner) = owner {
            if let (Ok(origin), size) = (owner.outer_position(), owner.outer_size()) {
                let target = window.outer_size();
                let mut x = origin.x + size.width as i32 + gap;
                let mut y = origin.y;
                if let Some(monitor) = owner.current_monitor() {
                    let monitor_origin = monitor.position();
                    let monitor_size = monitor.size();
                    let right = monitor_origin.x + monitor_size.width as i32;
                    let bottom = monitor_origin.y + monitor_size.height as i32;
                    if x + target.width as i32 > right {
                        x = origin.x - target.width as i32 - gap;
                    }
                    x = x.clamp(
                        monitor_origin.x,
                        (right - target.width as i32).max(monitor_origin.x),
                    );
                    y = y.clamp(
                        monitor_origin.y,
                        (bottom - target.height as i32).max(monitor_origin.y),
                    );
                }
                window.set_outer_position(PhysicalPosition::new(x, y));
                return;
            }
        }
    }
    if let WindowPosition::NearCursor { gap } = options.position {
        if let Some(cursor) = cursor {
            let target = window.outer_size();
            let mut x = cursor.x + gap;
            let mut y = cursor.y + gap;
            if let Some(monitor) = window.current_monitor() {
                let origin = monitor.position();
                let size = monitor.size();
                let right = origin.x + size.width as i32;
                let bottom = origin.y + size.height as i32;
                if x + target.width as i32 > right {
                    x = cursor.x - target.width as i32 - gap;
                }
                if y + target.height as i32 > bottom {
                    y = cursor.y - target.height as i32 - gap;
                }
                x = x.clamp(origin.x, (right - target.width as i32).max(origin.x));
                y = y.clamp(origin.y, (bottom - target.height as i32).max(origin.y));
            }
            window.set_outer_position(PhysicalPosition::new(x, y));
            return;
        }
    }
    if !matches!(options.position, WindowPosition::Centered) {
        return;
    }
    let Some(monitor) = window.current_monitor() else {
        return;
    };
    let origin = monitor.position();
    let area = monitor.size();
    let size = window.outer_size();
    window.set_outer_position(PhysicalPosition::new(
        origin.x + (area.width as i32 - size.width as i32) / 2,
        origin.y + (area.height as i32 - size.height as i32) / 2,
    ));
}
