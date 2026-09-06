use super::super::*;
use crate::{
    core::{group, Scene},
    renderer::{RenderStats, RendererCapabilities},
};
use windows::Win32::UI::WindowsAndMessaging::{SW_RESTORE, SW_SHOWMINNOACTIVE};

#[derive(Clone, Default)]
struct RecordingRenderer(Arc<Mutex<Vec<(PhysicalRect, bool)>>>);

impl Win32RendererFactory for RecordingRenderer {
    fn create(&self, _hwnd: HWND) -> Result<Box<Win32SceneRenderer>> {
        Ok(Box::new(self.clone()))
    }
}

impl SceneRenderer for RecordingRenderer {
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
        _scene: &Scene,
        frame: &FrameInfo<'_>,
    ) -> std::result::Result<RenderStats, Self::Error> {
        self.0
            .lock()
            .unwrap()
            .push((frame.viewport(), frame.is_full_redraw()));
        Ok(RenderStats::for_frame(frame))
    }
}

struct TestWindow {
    hwnd: HWND,
    _class: RegisteredWindowClass,
}

impl TestWindow {
    fn new(renderer: RecordingRenderer, builds: Arc<AtomicUsize>) -> Self {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }.unwrap().0);
        let class_name = wide("LguiRenderLifecycleTest");
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        assert_ne!(unsafe { RegisterClassExW(&class) }, 0);
        let class = RegisteredWindowClass {
            instance,
            class_name,
            icons: WindowClassIcons::default(),
        };
        let hwnd = create_window(
            instance,
            &class.class_name,
            WindowOptions::new("render-lifecycle")
                .native_titlebar(false)
                .resizable(false)
                .size(Size::new(640.0, 480.0))
                .visible(false),
            Arc::new(move |cx| {
                builds.fetch_add(1, Ordering::SeqCst);
                group(cx.viewport())
            }),
            ApplicationContext::empty(crate::memory::MemoryOptions::unbounded(
                crate::memory::ImageCachePolicy::NoStore,
                false,
            )),
            Arc::new(renderer),
            Win32Dispatcher::new(),
        )
        .unwrap();
        Self {
            hwnd,
            _class: class,
        }
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        unsafe {
            DestroyWindow(self.hwnd).unwrap();
        }
    }
}

#[test]
fn minimize_restore_preserves_layout_and_repaints_the_entire_window() {
    let renderer = RecordingRenderer::default();
    let builds = Arc::new(AtomicUsize::new(0));
    let window = TestWindow::new(renderer.clone(), Arc::clone(&builds));
    let hwnd = window.hwnd;
    // Hidden startup must still prime the first frame.
    let initial = renderer.0.lock().unwrap()[0];
    assert!(initial.0.width() > 1 && initial.0.height() > 1);
    show_window(hwnd);

    for cycle in 0..3 {
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWMINNOACTIVE);
        }
        assert!(unsafe { IsIconic(hwnd).as_bool() });
        STATE.with(|state| {
            let state = state.borrow();
            let state = &state[&(hwnd.0 as isize)];
            assert!(state.minimized);
            assert!(!state.can_advance_animations());
        });
        let frame_count = renderer.0.lock().unwrap().len();
        let build_count = builds.load(Ordering::SeqCst);
        if cycle > 0 {
            // Exercise both WM_PAINT and the direct rendering entry used for priming.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            paint(hwnd);
            render_hidden_window_once(hwnd);
        }
        assert_eq!(renderer.0.lock().unwrap().len(), frame_count);
        assert_eq!(builds.load(Ordering::SeqCst), build_count);

        renderer.0.lock().unwrap().clear();
        unsafe {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        render_hidden_window_once(hwnd);
        let frames = renderer.0.lock().unwrap();
        assert_eq!(frames[0], (initial.0, true), "restore cycle {cycle}");
        assert!(frames.iter().all(|frame| frame.0 == initial.0));
    }

    assert!(suspend_window_rendering(hwnd, true));
    let frame_count = renderer.0.lock().unwrap().len();
    render_hidden_window_once(hwnd);
    assert_eq!(renderer.0.lock().unwrap().len(), frame_count);
    resume_window_rendering(hwnd);
    render_hidden_window_once(hwnd);
    assert_eq!(renderer.0.lock().unwrap().last(), Some(&(initial.0, true)));
}

#[test]
fn iconic_transition_geometry_is_rejected_but_normal_negative_coordinates_are_valid() {
    let client = RECT {
        right: 640,
        bottom: 480,
        ..Default::default()
    };
    let normal = RECT {
        left: -1920,
        top: -1080,
        right: -1280,
        bottom: -600,
    };
    assert!(!should_skip_window_geometry(client, normal));
    for window in [
        RECT {
            left: -32000,
            ..normal
        },
        RECT {
            top: -32000,
            ..normal
        },
    ] {
        assert!(should_skip_window_geometry(client, window));
    }
    for invalid in [
        RECT::default(),
        RECT { right: 1, ..client },
        RECT {
            bottom: 1,
            ..client
        },
    ] {
        assert!(should_skip_window_geometry(invalid, normal));
    }
}
