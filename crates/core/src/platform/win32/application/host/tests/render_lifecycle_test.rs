use super::super::*;
use crate::{
    core::{group, Scene},
    renderer::{RenderStats, RendererCapabilities},
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindow, IsWindow, GW_HWNDNEXT, GW_HWNDPREV, GW_OWNER, HWND_TOP, SWP_NOMOVE, SWP_NOSIZE,
    SW_MINIMIZE, SW_RESTORE, SW_SHOWMINNOACTIVE, WS_OVERLAPPEDWINDOW,
};

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

struct OtherWindow(HWND);

impl Drop for OtherWindow {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.0);
        }
    }
}

fn is_above(window: HWND, other: HWND) -> bool {
    let mut next = unsafe { GetWindow(window, GW_HWNDNEXT) };
    while let Ok(hwnd) = next {
        if hwnd == other {
            return true;
        }
        next = unsafe { GetWindow(hwnd, GW_HWNDNEXT) };
    }
    false
}

impl TestWindow {
    fn new(renderer: RecordingRenderer, builds: Arc<AtomicUsize>) -> Self {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }.unwrap().0);
        static NEXT_CLASS_ID: AtomicUsize = AtomicUsize::new(0);
        let class_name = wide(&format!(
            "LguiRenderLifecycleTest.{}",
            NEXT_CLASS_ID.fetch_add(1, Ordering::SeqCst)
        ));
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
fn owner_drag_leaves_chat_position_visibility_and_z_order_unchanged() {
    let renderer = RecordingRenderer::default();
    let owner = TestWindow::new(renderer.clone(), Arc::new(AtomicUsize::new(0)));
    let context = STATE.with(|state| state.borrow()[&(owner.hwnd.0 as isize)].context.clone());
    let create_child = |id: &str, position| {
        create_window(
            owner._class.instance,
            &owner._class.class_name,
            WindowOptions::new(id)
                .owner("render-lifecycle")
                .native_titlebar(false)
                .resizable(false)
                .size(Size::new(120.0, 80.0))
                .position(position)
                .with_platform_options(
                    Win32WindowOptions::default()
                        .owner_z_order(id != "chat")
                        .minimize_with_owner(id != "chat"),
                )
                .visible(false),
            Arc::new(|cx| group(cx.viewport())),
            context.clone(),
            Arc::new(renderer.clone()),
            Win32Dispatcher::new(),
        )
        .unwrap()
    };
    let chat = create_child("chat", WindowPosition::Centered);
    let adjacent = create_child("friends", WindowPosition::AdjacentToOwner { gap: 1 });
    let hidden = create_child("hidden", WindowPosition::Centered);
    show_window(owner.hwnd);
    show_window(chat);
    show_window(adjacent);
    assert!(unsafe { GetWindow(chat, GW_OWNER) }.is_err());
    assert_eq!(
        unsafe { GetWindow(adjacent, GW_OWNER) }.unwrap(),
        owner.hwnd
    );
    let mut owner_rect = RECT::default();
    unsafe {
        GetWindowRect(owner.hwnd, &mut owner_rect).unwrap();
        SetWindowPos(
            chat,
            Some(owner.hwnd),
            owner_rect.left + 24,
            owner_rect.top + 32,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE,
        )
        .unwrap();
    }
    let mut chat_rect = RECT::default();
    unsafe {
        GetWindowRect(chat, &mut chat_rect).unwrap();
    }

    for _ in 0..2 {
        window_proc(owner.hwnd, WM_ENTERSIZEMOVE, WPARAM(0), LPARAM(0));
        window_proc(owner.hwnd, WM_MOVING, WPARAM(0), LPARAM(0));
        owner_rect.left += 10;
        owner_rect.top += 10;
        unsafe {
            SetWindowPos(
                owner.hwnd,
                None,
                owner_rect.left,
                owner_rect.top,
                0,
                0,
                SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER,
            )
            .unwrap();
        }
        let mut current_chat_rect = RECT::default();
        unsafe {
            GetWindowRect(chat, &mut current_chat_rect).unwrap();
        }
        assert_eq!(current_chat_rect, chat_rect);
        assert_eq!(unsafe { GetWindow(chat, GW_HWNDPREV) }.unwrap(), owner.hwnd);
        assert!(unsafe { IsWindowVisible(chat).as_bool() });
        assert!(!unsafe { IsWindowVisible(adjacent).as_bool() });
        assert!(!unsafe { IsWindowVisible(hidden).as_bool() });
        unsafe {
            GetWindowRect(chat, &mut current_chat_rect).unwrap();
        }
        assert_eq!(current_chat_rect, chat_rect);
        window_proc(owner.hwnd, WM_EXITSIZEMOVE, WPARAM(0), LPARAM(0));
        assert!(unsafe { IsWindowVisible(chat).as_bool() });
        assert!(unsafe { IsWindowVisible(adjacent).as_bool() });
        assert!(!unsafe { IsWindowVisible(hidden).as_bool() });
    }

    let other = OtherWindow(unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::w!("STATIC"),
            windows::core::w!("Other application window"),
            WS_OVERLAPPEDWINDOW,
            0,
            0,
            160,
            120,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    });
    unsafe {
        let _ = ShowWindow(other.0, SW_SHOW);
        let _ = ShowWindow(chat, SW_SHOW);
        let _ = ShowWindow(owner.hwnd, SW_SHOW);
        SetWindowPos(chat, Some(HWND_TOP), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE).unwrap();
        SetWindowPos(
            owner.hwnd,
            Some(HWND_TOP),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE,
        )
        .unwrap();
    }
    assert!(is_above(owner.hwnd, other.0));
    assert!(is_above(chat, other.0));
    unsafe {
        let _ = ShowWindow(owner.hwnd, SW_MINIMIZE);
    }
    assert!(
        is_above(chat, other.0),
        "minimizing the owner must not put another window above chat"
    );
    assert!(unsafe { IsWindowVisible(chat).as_bool() });
    assert!(!unsafe { IsIconic(chat).as_bool() });
    let mut current_chat_rect = RECT::default();
    unsafe {
        GetWindowRect(chat, &mut current_chat_rect).unwrap();
    }
    assert_eq!(current_chat_rect, chat_rect);
    assert!(!unsafe { IsWindowVisible(adjacent).as_bool() });
    unsafe {
        let _ = ShowWindow(owner.hwnd, SW_RESTORE);
    }
    assert!(unsafe { IsWindowVisible(chat).as_bool() });
    assert!(unsafe { IsWindowVisible(adjacent).as_bool() });
    assert!(!unsafe { IsWindowVisible(hidden).as_bool() });

    hide_window(chat);
    unsafe {
        let _ = ShowWindow(owner.hwnd, SW_SHOWMINNOACTIVE);
        let _ = ShowWindow(owner.hwnd, SW_RESTORE);
    }
    assert!(!unsafe { IsWindowVisible(chat).as_bool() });
    window_proc(owner.hwnd, WM_ENTERSIZEMOVE, WPARAM(0), LPARAM(0));
    window_proc(owner.hwnd, WM_EXITSIZEMOVE, WPARAM(0), LPARAM(0));
    assert!(!unsafe { IsWindowVisible(chat).as_bool() });
    drop(owner);
    assert!(!unsafe { IsWindow(Some(chat)).as_bool() });
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
