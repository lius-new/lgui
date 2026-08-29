use std::{ffi::CString, mem::ManuallyDrop, num::NonZeroU32, sync::Arc};

use glutin::{
    config::{Config, ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext,
        PossiblyCurrentGlContext, Version,
    },
    display::{Display, DisplayApiPreference, GlDisplay},
    surface::{
        GlSurface, Surface as GlutinSurface, SurfaceAttributesBuilder, SwapInterval, WindowSurface,
    },
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawWindowHandle};
use skia_safe::{
    gpu::{self, backend_render_targets, direct_contexts, gl::FramebufferInfo, SurfaceOrigin},
    Color as SkColor, ColorType, ImageInfo, Paint, Rect, Surface,
};
use winit::window::Window;

use crate::{
    core::{PhysicalRect, Scene},
    platform::skia::{paint_scene_damage, SkiaCache, SkiaCacheStats},
    renderer::{FrameInfo, MemoryPressure},
};

pub(crate) struct WinitOpenGlRenderer {
    window_surface: ManuallyDrop<Surface>,
    scene_surface: ManuallyDrop<Surface>,
    skia_context: ManuallyDrop<gpu::DirectContext>,
    framebuffer: FramebufferInfo,
    size: (i32, i32),
    cache: SkiaCache,
    gl_surface: GlutinSurface<WindowSurface>,
    gl_context: PossiblyCurrentContext,
    _display: Display,
    _config: Config,
    _window: Arc<Window>,
}

impl WinitOpenGlRenderer {
    pub(crate) fn new(
        window: Arc<Window>,
        cache_budget: usize,
        transparent: bool,
    ) -> Result<Self, String> {
        let raw_window = window
            .window_handle()
            .map_err(|error| format!("read OpenGL window handle: {error}"))?
            .as_raw();
        let raw_display = window
            .display_handle()
            .map_err(|error| format!("read OpenGL display handle: {error}"))?
            .as_raw();
        let display = unsafe { Display::new(raw_display, display_preference(raw_window)) }
            .map_err(|error| format!("create OpenGL display: {error}"))?;
        let template = ConfigTemplateBuilder::new()
            .with_alpha_size(if transparent { 8 } else { 0 })
            .with_stencil_size(8)
            .with_transparency(transparent)
            .prefer_hardware_accelerated(Some(true))
            .compatible_with_native_window(raw_window)
            .build();
        let config = unsafe { display.find_configs(template) }
            .map_err(|error| format!("enumerate OpenGL configurations: {error}"))?
            .max_by_key(|config| {
                usize::from(config.hardware_accelerated()) * 1000
                    + config.num_samples() as usize
                    + config.alpha_size() as usize
            })
            .ok_or_else(|| "no compatible OpenGL window configuration".to_owned())?;
        if transparent
            && (config.alpha_size() == 0 || config.supports_transparency() == Some(false))
        {
            return Err(
                "the selected OpenGL window configuration does not support transparency".to_owned(),
            );
        }

        let attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 2))))
            .build(Some(raw_window));
        let fallback_attributes = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::Gles(Some(Version::new(3, 0))))
            .build(Some(raw_window));
        let not_current = unsafe { display.create_context(&config, &attributes) }
            .or_else(|_| unsafe { display.create_context(&config, &fallback_attributes) })
            .map_err(|error| format!("create OpenGL context: {error}"))?;
        let size = window.inner_size();
        let width = NonZeroU32::new(size.width.max(1)).unwrap();
        let height = NonZeroU32::new(size.height.max(1)).unwrap();
        let surface_attributes =
            SurfaceAttributesBuilder::<WindowSurface>::new().build(raw_window, width, height);
        let gl_surface = unsafe { display.create_window_surface(&config, &surface_attributes) }
            .map_err(|error| format!("create OpenGL window surface: {error}"))?;
        let gl_context = not_current
            .make_current(&gl_surface)
            .map_err(|error| format!("make OpenGL context current: {error}"))?;
        let _ = gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        let interface = skia_safe::gpu::gl::Interface::new_load_with(|name| {
            let Ok(name) = CString::new(name) else {
                return std::ptr::null();
            };
            display.get_proc_address(&name)
        })
        .ok_or_else(|| "Skia could not load the current OpenGL interface".to_owned())?;
        let mut skia_context = direct_contexts::make_gl(interface, None)
            .ok_or_else(|| "Skia could not create an OpenGL DirectContext".to_owned())?;
        skia_context.set_resource_cache_limit(cache_budget);
        let framebuffer = FramebufferInfo {
            fboid: 0,
            format: skia_safe::gpu::gl::Format::RGBA8.into(),
            ..Default::default()
        };
        let size = (width.get() as i32, height.get() as i32);
        let (window_surface, scene_surface) =
            create_gpu_surfaces(&mut skia_context, framebuffer, size)?;
        Ok(Self {
            window_surface: ManuallyDrop::new(window_surface),
            scene_surface: ManuallyDrop::new(scene_surface),
            skia_context: ManuallyDrop::new(skia_context),
            framebuffer,
            size,
            cache: SkiaCache::new(cache_budget),
            gl_surface,
            gl_context,
            _display: display,
            _config: config,
            _window: window,
        })
    }

    fn ensure_current_and_sized(&mut self, viewport: PhysicalRect) -> Result<(), String> {
        self.gl_context
            .make_current(&self.gl_surface)
            .map_err(|error| format!("make OpenGL context current during render: {error}"))?;
        let size = (viewport.width().max(1), viewport.height().max(1));
        if size != self.size {
            self.skia_context.flush_and_submit();
            self.gl_surface.resize(
                &self.gl_context,
                NonZeroU32::new(size.0 as u32).unwrap(),
                NonZeroU32::new(size.1 as u32).unwrap(),
            );
            let (window_surface, scene_surface) =
                create_gpu_surfaces(&mut self.skia_context, self.framebuffer, size)?;
            unsafe {
                ManuallyDrop::drop(&mut self.window_surface);
                ManuallyDrop::drop(&mut self.scene_surface);
            }
            self.window_surface = ManuallyDrop::new(window_surface);
            self.scene_surface = ManuallyDrop::new(scene_surface);
            self.size = size;
            self.cache.trim(MemoryPressure::Critical);
        }
        Ok(())
    }

    pub(crate) fn draw_scene(
        &mut self,
        scene: &Scene,
        frame: &FrameInfo<'_>,
    ) -> Result<(), String> {
        self.ensure_current_and_sized(frame.viewport())?;
        paint_scene_damage(self.scene_surface.canvas(), &mut self.cache, scene, frame)?;
        let image = self.scene_surface.image_snapshot();
        let destination = Rect::from_wh(self.size.0 as f32, self.size.1 as f32);
        self.window_surface.canvas().clear(SkColor::TRANSPARENT);
        self.window_surface
            .canvas()
            .draw_image_rect(image, None, &destination, &Paint::default());
        self.skia_context.flush_and_submit();
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .map_err(|error| format!("swap OpenGL buffers: {error}"))
    }

    pub(crate) fn trim(&mut self, pressure: MemoryPressure) {
        self.cache.trim(pressure);
        if pressure == MemoryPressure::Critical {
            self.skia_context.free_gpu_resources();
        }
    }

    pub(crate) fn cache_stats(&self) -> SkiaCacheStats {
        self.cache.stats()
    }
}

impl Drop for WinitOpenGlRenderer {
    fn drop(&mut self) {
        let context_is_current = self.gl_context.make_current(&self.gl_surface).is_ok();
        if !context_is_current {
            self.skia_context.abandon();
        }
        unsafe {
            ManuallyDrop::drop(&mut self.window_surface);
            ManuallyDrop::drop(&mut self.scene_surface);
            if context_is_current {
                self.skia_context.release_resources_and_abandon();
            }
            ManuallyDrop::drop(&mut self.skia_context);
        }
        if context_is_current {
            let _ = self.gl_context.make_not_current_in_place();
        }
    }
}

fn create_gpu_surfaces(
    context: &mut gpu::DirectContext,
    framebuffer: FramebufferInfo,
    size: (i32, i32),
) -> Result<(Surface, Surface), String> {
    let target = backend_render_targets::make_gl(size, 0, 8, framebuffer);
    let window = gpu::surfaces::wrap_backend_render_target(
        context,
        &target,
        SurfaceOrigin::BottomLeft,
        ColorType::RGBA8888,
        None,
        None,
    )
    .ok_or_else(|| "Skia could not wrap the OpenGL framebuffer".to_owned())?;
    let info = ImageInfo::new_n32_premul(size, None);
    let scene = gpu::surfaces::render_target(
        context,
        gpu::Budgeted::Yes,
        &info,
        None,
        SurfaceOrigin::TopLeft,
        None,
        false,
        false,
    )
    .ok_or_else(|| "Skia could not create the retained GPU scene surface".to_owned())?;
    Ok((window, scene))
}

#[cfg(target_os = "windows")]
fn display_preference(raw_window: RawWindowHandle) -> DisplayApiPreference {
    DisplayApiPreference::Wgl(Some(raw_window))
}

#[cfg(target_os = "linux")]
fn display_preference(_raw_window: RawWindowHandle) -> DisplayApiPreference {
    DisplayApiPreference::Egl
}

#[cfg(target_os = "macos")]
fn display_preference(_raw_window: RawWindowHandle) -> DisplayApiPreference {
    DisplayApiPreference::Cgl
}
