#[cfg(feature = "diagnostics")]
use std::time::Instant;
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    mem::size_of,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use windows::{
    core::{Error, Result, HRESULT, PCWSTR},
    Win32::{
        Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
        Graphics::{
            Dwm::{
                DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
                DWMWCP_ROUND, DWM_WINDOW_CORNER_PREFERENCE,
            },
            Gdi::{
                BeginPaint, EndPaint, GetDC, GetMonitorInfoW, InvalidateRect, MonitorFromWindow,
                ReleaseDC, ScreenToClient, UpdateWindow, HDC, MONITORINFO,
                MONITOR_DEFAULTTONEAREST, PAINTSTRUCT,
            },
        },
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{
                GetDpiForWindow, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
                DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
            },
            Input::{
                Ime::{
                    ImmGetCompositionStringW, ImmGetContext, ImmReleaseContext,
                    ImmSetCompositionWindow, CFS_POINT, COMPOSITIONFORM, GCS_COMPSTR,
                    GCS_CURSORPOS, GCS_RESULTSTR,
                },
                KeyboardAndMouse::{
                    GetKeyState, ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE,
                    TRACKMOUSEEVENT, VK_CONTROL, VK_SHIFT,
                },
            },
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyCaret, DestroyIcon, DestroyWindow,
                DispatchMessageW, GetClassLongPtrW, GetClientRect, GetCursorPos, GetMessageW,
                GetSystemMetrics, GetWindowLongPtrW, GetWindowPlacement, GetWindowRect,
                IsWindowVisible, IsZoomed, LoadCursorW, PostMessageW, PostQuitMessage,
                RegisterClassExW, SetLayeredWindowAttributes, SetWindowLongPtrW,
                SetWindowPlacement, SetWindowPos, ShowWindow, TranslateMessage, UnregisterClassW,
                CW_USEDEFAULT, GCLP_HICON, GCLP_HICONSM, GWL_STYLE, HICON, HTBOTTOM, HTBOTTOMLEFT,
                HTBOTTOMRIGHT, HTCAPTION, HTCLIENT, HTLEFT, HTRIGHT, HTTOP, HTTOPLEFT, HTTOPRIGHT,
                ICON_BIG, ICON_SMALL, IDC_ARROW, LWA_ALPHA, MINMAXINFO, MSG, SIZE_MINIMIZED,
                SM_CXICON, SM_CXPADDEDBORDER, SM_CXSIZEFRAME, SM_CXSMICON, SM_CYICON,
                SM_CYSIZEFRAME, SM_CYSMICON, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOOWNERZORDER,
                SWP_NOZORDER, SW_HIDE, SW_SHOW, WA_INACTIVE, WINDOWPLACEMENT, WINDOW_EX_STYLE,
                WINDOW_STYLE, WM_ACTIVATE, WM_CHAR, WM_CLOSE, WM_DESTROY, WM_DPICHANGED,
                WM_ENTERSIZEMOVE, WM_ERASEBKGND, WM_EXITSIZEMOVE, WM_GETMINMAXINFO,
                WM_IME_COMPOSITION, WM_IME_ENDCOMPOSITION, WM_IME_STARTCOMPOSITION, WM_KEYDOWN,
                WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
                WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_MOVE, WM_MOVING, WM_NCCALCSIZE,
                WM_NCHITTEST, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETICON, WM_SIZE,
                WM_SIZING, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW,
                WS_CAPTION, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_MAXIMIZEBOX,
                WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
            },
        },
    },
};

#[cfg(feature = "notifications")]
use crate::application::NotificationRegistration;
#[cfg(feature = "tray")]
use crate::application::TrayRegistration;
#[cfg(all(
    feature = "diagnostics",
    feature = "advanced-rendering",
    feature = "renderer-gdi"
))]
use crate::diagnostics::FrameBlitSourceMetrics;
#[cfg(feature = "notifications")]
use crate::platform::{NotificationError, NotificationHandle};
#[cfg(feature = "renderer-gdi")]
use crate::renderer::{RenderStats, RendererCapabilities};
use crate::{
    application::{
        application_root_view, AppView, ApplicationBackend, ApplicationContext, ClosePolicy,
        RenderError, WindowCloseHandler, WindowCommand, WindowDragExclusion, WindowId, WindowMode,
        WindowOptions, WindowPosition,
    },
    core::{
        dispatch_runtime_output, ImeEvent, InputEvent, KeyLocation, KeyModifiers, KeyState,
        KeyboardEvent, LogicalKey, NamedKey, PhysicalKey, PhysicalPoint, PhysicalRect,
        PhysicalSize, Point, PointerButton, PointerData, RuntimeOutput, Size, UiEvent, UiRect,
        WheelDelta,
    },
    renderer::{FrameInfo, FrameReason, RenderErrorStage, SceneRenderer},
    session::UiSession,
};
#[cfg(feature = "diagnostics")]
use crate::{
    diagnostics::{
        duration_ms, DiagnosticPresentMode, DiagnosticsRegistration, FramePresentMetrics,
        FrameRenderMetrics, FrameSample,
    },
    host::DamageReason,
};

use super::super::ico::create_icon_from_ico_bytes;

include!("host/contract.rs");
include!("host/state.rs");
include!("host/backend.rs");
include!("host/window.rs");
include!("host/message_loop.rs");
include!("host/rendering.rs");
include!("host/input.rs");
include!("host/tests.rs");
