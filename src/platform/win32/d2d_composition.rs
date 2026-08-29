use std::mem::ManuallyDrop;

use windows::{
    core::{Error, Interface, Result, HRESULT},
    Win32::{
        Foundation::{HMODULE, HWND, RECT},
        Graphics::{
            Direct2D::{
                Common::{D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_PIXEL_FORMAT},
                D2D1CreateFactory, ID2D1Bitmap1, ID2D1ColorContext, ID2D1Device,
                ID2D1DeviceContext, ID2D1Factory1, D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
                D2D1_BITMAP_OPTIONS_TARGET, D2D1_BITMAP_PROPERTIES1,
                D2D1_DEVICE_CONTEXT_OPTIONS_NONE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
            },
            Direct3D::{
                D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL, D3D_FEATURE_LEVEL_10_0,
                D3D_FEATURE_LEVEL_10_1, D3D_FEATURE_LEVEL_11_0, D3D_FEATURE_LEVEL_11_1,
            },
            Direct3D11::{
                D3D11CreateDevice, ID3D11Device, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                D3D11_SDK_VERSION,
            },
            DirectComposition::{
                DCompositionCreateDevice, IDCompositionDevice, IDCompositionTarget,
                IDCompositionVisual,
            },
            DirectWrite::{DWriteCreateFactory, IDWriteFactory, DWRITE_FACTORY_TYPE_SHARED},
            Dxgi::{
                Common::{
                    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC,
                },
                CreateDXGIFactory1, IDXGIDevice, IDXGIFactory2, IDXGISurface, IDXGISwapChain,
                IDXGISwapChain1, DXGI_PRESENT, DXGI_PRESENT_PARAMETERS, DXGI_SCALING_STRETCH,
                DXGI_SWAP_CHAIN_DESC1, DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
                DXGI_USAGE_RENDER_TARGET_OUTPUT,
            },
        },
    },
};

use crate::{
    core::{PhysicalRect, Scene},
    renderer::{FrameInfo, RenderErrorStage, RenderStats, RendererCapabilities, SceneRenderer},
};

use super::super::{
    enhanced, Win32RenderError, Win32RenderTarget, Win32RendererFactory, Win32SceneRenderer,
};

#[derive(Clone, Copy, Debug, Default)]
pub struct D2dRendererFactory;

impl Win32RendererFactory for D2dRendererFactory {
    fn name(&self) -> &'static str {
        "d2d"
    }

    fn create(&self, hwnd: HWND) -> Result<Box<Win32SceneRenderer>> {
        Ok(Box::new(D2dRenderer::new(hwnd)))
    }
}

pub fn probe_d2d_support() -> Result<()> {
    let (d3d_device, _) = create_d3d_device()?;
    let dxgi_device: IDXGIDevice = d3d_device.cast()?;
    let d2d_factory: ID2D1Factory1 =
        unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
    let d2d_device: ID2D1Device = unsafe { d2d_factory.CreateDevice(&dxgi_device) }?;
    let context = unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) }?;
    let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
    let _renderer = enhanced::d2d::D2dRenderer::new(context, dwrite, 1, 1)?;
    Ok(())
}

pub struct D2dRenderer {
    hwnd: HWND,
    size: (i32, i32),
    resources: Option<CompositionResources>,
}

impl D2dRenderer {
    fn new(hwnd: HWND) -> Self {
        Self {
            hwnd,
            size: (0, 0),
            resources: None,
        }
    }

    fn ensure_resources(&mut self, viewport: PhysicalRect) -> Result<&mut CompositionResources> {
        let size = (viewport.width().max(1), viewport.height().max(1));
        if self.resources.is_none() || self.size != size {
            self.resources = Some(CompositionResources::new(self.hwnd, size.0, size.1)?);
            self.size = size;
        }
        self.resources
            .as_mut()
            .ok_or_else(|| Error::from_hresult(HRESULT(0x80004003_u32 as i32)))
    }

    fn resources_need_reset(&self, viewport: PhysicalRect) -> bool {
        let size = (viewport.width().max(1), viewport.height().max(1));
        self.resources.is_none() || self.size != size
    }
}

impl SceneRenderer for D2dRenderer {
    type Target = Win32RenderTarget;
    type Error = Win32RenderError;

    fn capabilities(&self) -> RendererCapabilities {
        RendererCapabilities {
            partial_redraw: true,
            retained_surface: true,
        }
    }

    fn render(
        &mut self,
        _target: &mut Self::Target,
        scene: &Scene,
        frame: &FrameInfo<'_>,
    ) -> std::result::Result<RenderStats, Self::Error> {
        let viewport = frame.viewport();
        let damage = frame.damage();
        let reset = self.resources_need_reset(viewport);
        if !reset && damage.is_empty() {
            // DirectComposition retains the last presented surface for exposure paints.
            return Ok(RenderStats::for_frame(frame));
        }
        let full = reset || frame.is_full_redraw() || damage_is_full(viewport, damage);
        let result = self
            .ensure_resources(viewport)
            .map_err(|source| {
                Win32RenderError::new(
                    RenderErrorStage::Create,
                    "create_composition_resources",
                    source,
                )
            })
            .and_then(|resources| {
                resources.draw_and_present(scene, if full { None } else { Some(damage) })
            });
        if result.is_err() {
            self.resources = None;
        }
        result?;
        Ok(RenderStats::for_frame(frame))
    }
}

struct CompositionResources {
    _d3d_device: ID3D11Device,
    swap_chain: IDXGISwapChain1,
    _d2d_context: ID2D1DeviceContext,
    dcomp_device: IDCompositionDevice,
    _dcomp_target: IDCompositionTarget,
    _dcomp_visual: IDCompositionVisual,
    target_bitmap: ID2D1Bitmap1,
    renderer: enhanced::d2d::D2dRenderer,
}

impl CompositionResources {
    fn new(hwnd: HWND, width: i32, height: i32) -> Result<Self> {
        let (d3d_device, _) = create_d3d_device()?;
        let dxgi_device: IDXGIDevice = d3d_device.cast()?;
        let dxgi_factory: IDXGIFactory2 = unsafe { CreateDXGIFactory1() }?;
        let swap_chain = unsafe {
            dxgi_factory.CreateSwapChainForComposition(
                &d3d_device,
                &swap_chain_desc(width, height),
                None,
            )
        }?;

        let d2d_factory: ID2D1Factory1 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None) }?;
        let d2d_device: ID2D1Device = unsafe { d2d_factory.CreateDevice(&dxgi_device) }?;
        let d2d_context =
            unsafe { d2d_device.CreateDeviceContext(D2D1_DEVICE_CONTEXT_OPTIONS_NONE) }?;
        let target_bitmap = create_target_bitmap(&d2d_context, &swap_chain)?;

        let dcomp_device: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi_device) }?;
        let dcomp_target = unsafe { dcomp_device.CreateTargetForHwnd(hwnd, true) }?;
        let dcomp_visual = unsafe { dcomp_device.CreateVisual() }?;
        unsafe {
            dcomp_visual.SetContent(&swap_chain)?;
            dcomp_target.SetRoot(&dcomp_visual)?;
            dcomp_device.Commit()?;
        }

        let dwrite: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED) }?;
        let renderer = enhanced::d2d::D2dRenderer::new(d2d_context.clone(), dwrite, width, height)?;

        Ok(Self {
            _d3d_device: d3d_device,
            swap_chain,
            _d2d_context: d2d_context,
            dcomp_device,
            _dcomp_target: dcomp_target,
            _dcomp_visual: dcomp_visual,
            target_bitmap,
            renderer,
        })
    }

    fn draw_and_present(
        &mut self,
        scene: &Scene,
        damage: Option<&[PhysicalRect]>,
    ) -> std::result::Result<(), Win32RenderError> {
        match damage {
            Some(rects) => {
                let scene_damage = rects
                    .iter()
                    .copied()
                    .map(PhysicalRect::as_ui_rect)
                    .collect::<Vec<_>>();
                self.renderer
                    .draw_scene_dirty(scene, &scene_damage)
                    .map_err(|source| {
                        Win32RenderError::new(RenderErrorStage::Draw, "draw_scene_dirty", source)
                    })?;
                self.renderer
                    .copy_scene_to_target(&self.target_bitmap, Some(&scene_damage))
                    .map_err(|source| {
                        Win32RenderError::new(
                            RenderErrorStage::Copy,
                            "copy_dirty_scene_to_swap_chain",
                            source,
                        )
                    })?;
            }
            None => {
                self.renderer.draw_scene_full(scene).map_err(|source| {
                    Win32RenderError::new(RenderErrorStage::Draw, "draw_scene_full", source)
                })?;
                self.renderer
                    .copy_scene_to_target(&self.target_bitmap, None)
                    .map_err(|source| {
                        Win32RenderError::new(
                            RenderErrorStage::Copy,
                            "copy_full_scene_to_swap_chain",
                            source,
                        )
                    })?;
            }
        }
        unsafe {
            match damage {
                Some(rects) => present_dirty(&self.swap_chain, rects).map_err(|source| {
                    Win32RenderError::new(RenderErrorStage::Present, "present_dirty", source)
                })?,
                None => self
                    .swap_chain
                    .cast::<IDXGISwapChain>()
                    .map_err(|source| {
                        Win32RenderError::new(
                            RenderErrorStage::Present,
                            "resolve_swap_chain_for_present",
                            source,
                        )
                    })?
                    .Present(1, DXGI_PRESENT(0))
                    .ok()
                    .map_err(|source| {
                        Win32RenderError::new(RenderErrorStage::Present, "present_full", source)
                    })?,
            }
            self.dcomp_device.Commit().map_err(|source| {
                Win32RenderError::new(
                    RenderErrorStage::Commit,
                    "commit_direct_composition",
                    source,
                )
            })?;
        }
        Ok(())
    }
}

fn damage_is_full(viewport: PhysicalRect, damage: &[PhysicalRect]) -> bool {
    damage.len() == 1 && damage[0] == viewport
}

unsafe fn present_dirty(swap_chain: &IDXGISwapChain1, damage: &[PhysicalRect]) -> Result<()> {
    let mut rects = damage
        .iter()
        .map(|rect| RECT {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        })
        .collect::<Vec<_>>();
    let parameters = DXGI_PRESENT_PARAMETERS {
        DirtyRectsCount: rects.len() as u32,
        pDirtyRects: rects.as_mut_ptr(),
        ..Default::default()
    };
    unsafe { swap_chain.Present1(1, DXGI_PRESENT(0), &parameters).ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_viewport_damage_uses_the_full_present_path() {
        let viewport = PhysicalRect::new(0, 0, 1280, 720);

        assert!(damage_is_full(viewport, &[viewport]));
    }

    #[test]
    fn partial_or_split_damage_keeps_the_dirty_present_path() {
        let viewport = PhysicalRect::new(0, 0, 1280, 720);

        assert!(!damage_is_full(
            viewport,
            &[PhysicalRect::new(12, 20, 240, 180)]
        ));
        assert!(!damage_is_full(
            viewport,
            &[
                PhysicalRect::new(0, 0, 640, 720),
                PhysicalRect::new(640, 0, 1280, 720),
            ]
        ));
    }
}

fn create_d3d_device() -> Result<(ID3D11Device, D3D_FEATURE_LEVEL)> {
    let feature_levels = [
        D3D_FEATURE_LEVEL_11_1,
        D3D_FEATURE_LEVEL_11_0,
        D3D_FEATURE_LEVEL_10_1,
        D3D_FEATURE_LEVEL_10_0,
    ];
    let mut device = None;
    let mut feature_level = D3D_FEATURE_LEVEL::default();
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            Some(&feature_levels),
            D3D11_SDK_VERSION,
            Some(&mut device),
            Some(&mut feature_level),
            None,
        )?;
    }
    let device = device.ok_or_else(|| Error::from_hresult(HRESULT(0x80004003_u32 as i32)))?;
    Ok((device, feature_level))
}

fn swap_chain_desc(width: i32, height: i32) -> DXGI_SWAP_CHAIN_DESC1 {
    DXGI_SWAP_CHAIN_DESC1 {
        Width: width.max(1) as u32,
        Height: height.max(1) as u32,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        Stereo: false.into(),
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
        BufferCount: 2,
        Scaling: DXGI_SCALING_STRETCH,
        SwapEffect: DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
        AlphaMode: DXGI_ALPHA_MODE_PREMULTIPLIED,
        Flags: 0,
    }
}

fn create_target_bitmap(
    context: &ID2D1DeviceContext,
    swap_chain: &IDXGISwapChain1,
) -> Result<ID2D1Bitmap1> {
    let surface: IDXGISurface = unsafe { swap_chain.cast::<IDXGISwapChain>()?.GetBuffer(0) }?;
    let properties = D2D1_BITMAP_PROPERTIES1 {
        pixelFormat: D2D1_PIXEL_FORMAT {
            format: DXGI_FORMAT_B8G8R8A8_UNORM,
            alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
        },
        dpiX: 96.0,
        dpiY: 96.0,
        bitmapOptions: D2D1_BITMAP_OPTIONS_TARGET | D2D1_BITMAP_OPTIONS_CANNOT_DRAW,
        colorContext: ManuallyDrop::new(None::<ID2D1ColorContext>),
    };
    unsafe { context.CreateBitmapFromDxgiSurface(&surface, Some(&properties)) }
}
