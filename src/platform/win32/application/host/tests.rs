use std::collections::VecDeque;

use windows::Win32::Foundation::RECT;

#[cfg(feature = "images-win32")]
use super::image_repaint_bounds;

use super::{
    can_advance_window_animations, decode_utf16_char_unit, resize_border_hit,
    suppress_committed_ime_char, window_corner_preference, window_style, OwnerVisibility,
    PhysicalPoint, ResizeFrameThrottle, Win32WindowOptions, WindowInteractionMode, WindowMode,
    WindowOptions, DWMWCP_DONOTROUND, DWMWCP_ROUND, HTBOTTOMRIGHT, HTCLIENT, HTTOPLEFT, WS_CAPTION,
    WS_POPUP, WS_THICKFRAME,
};

#[test]
fn committed_ime_units_are_suppressed_until_the_sequence_diverges() {
    let mut pending = "\u{4E2D}\u{6587}".encode_utf16().collect::<VecDeque<_>>();
    assert!(suppress_committed_ime_char(&mut pending, '\u{4E2D}' as u16));
    assert!(!suppress_committed_ime_char(&mut pending, 'x' as u16));
    assert!(pending.is_empty());
}

#[test]
fn utf16_decoder_combines_a_surrogate_pair() {
    let mut high = None;
    assert_eq!(decode_utf16_char_unit(&mut high, 0xD83D), None);
    assert_eq!(
        decode_utf16_char_unit(&mut high, 0xDE00),
        Some("\u{1F600}".to_string())
    );
    assert_eq!(high, None);
}

#[test]
fn corner_preferences_map_to_explicit_dwm_requests() {
    assert_eq!(
        window_corner_preference(0, WindowMode::Windowed),
        DWMWCP_DONOTROUND
    );
    assert_eq!(
        window_corner_preference(4, WindowMode::Windowed),
        windows::Win32::Graphics::Dwm::DWMWCP_ROUNDSMALL
    );
    assert_eq!(
        window_corner_preference(8, WindowMode::Windowed),
        DWMWCP_ROUND
    );
    assert_eq!(
        window_corner_preference(8, WindowMode::Fullscreen),
        DWMWCP_DONOTROUND
    );
}

#[test]
fn win32_window_options_own_class_and_icon_policy() {
    static ICON: &[u8] = b"icon";
    let defaults = Win32WindowOptions::default();
    assert_eq!(defaults.class_name, None);
    assert_eq!(defaults.icon_bytes, None);

    let custom = Win32WindowOptions::default()
        .class_name("Example.Window")
        .icon_bytes(ICON);
    assert_eq!(custom.class_name.as_deref(), Some("Example.Window"));
    assert_eq!(custom.icon_bytes, Some(ICON));
}

#[test]
fn custom_frames_remove_the_caption_and_keep_only_requested_resize_capability() {
    let decorated = window_style(&WindowOptions::new("decorated"));
    let custom = window_style(&WindowOptions::new("custom").native_titlebar(false));
    let fixed = window_style(
        &WindowOptions::new("fixed")
            .native_titlebar(false)
            .resizable(false),
    );

    assert_ne!(decorated.0 & WS_CAPTION.0, 0);
    assert_eq!(decorated.0 & WS_POPUP.0, 0);
    assert_eq!(custom.0 & WS_CAPTION.0, 0);
    assert_ne!(custom.0 & WS_POPUP.0, 0);
    assert_ne!(custom.0 & WS_THICKFRAME.0, 0);
    assert_eq!(fixed.0 & WS_CAPTION.0, 0);
    assert_ne!(fixed.0 & WS_POPUP.0, 0);
    assert_eq!(fixed.0 & WS_THICKFRAME.0, 0);
}

#[test]
fn custom_frame_resize_hit_testing_includes_edges_and_corners() {
    let rect = RECT {
        left: 100,
        top: 200,
        right: 900,
        bottom: 700,
    };

    assert_eq!(
        resize_border_hit(rect, PhysicalPoint::new(101, 201), 8, 8),
        HTTOPLEFT
    );
    assert_eq!(
        resize_border_hit(rect, PhysicalPoint::new(899, 699), 8, 8),
        HTBOTTOMRIGHT
    );
    assert_eq!(
        resize_border_hit(rect, PhysicalPoint::new(500, 400), 8, 8),
        HTCLIENT
    );
}

#[test]
fn owner_hide_and_restore_preserve_requested_visibility() {
    let mut visible_child = OwnerVisibility::visible();
    assert!(visible_child.hide_for_owner());
    assert!(visible_child.hidden_for_owner);
    assert!(visible_child.restore_for_owner());
    assert!(!visible_child.hidden_for_owner);
    assert!(visible_child.desired_visible);

    let mut explicitly_hidden_child = OwnerVisibility::visible();
    explicitly_hidden_child.set_desired(false);
    assert!(!explicitly_hidden_child.hide_for_owner());
    assert!(!explicitly_hidden_child.restore_for_owner());
    assert!(!explicitly_hidden_child.desired_visible);
    assert!(!explicitly_hidden_child.hidden_for_owner);
}

#[test]
fn explicit_hide_while_owner_is_hidden_prevents_restore() {
    let mut child = OwnerVisibility::visible();
    assert!(child.hide_for_owner());
    child.set_desired(false);

    assert!(!child.restore_for_owner());
    assert!(!child.desired_visible);
    assert!(!child.hidden_for_owner);
}

#[test]
fn only_visible_idle_windows_advance_animations() {
    let visible = OwnerVisibility::visible();
    assert!(can_advance_window_animations(
        false,
        visible,
        WindowInteractionMode::Idle
    ));
    assert!(!can_advance_window_animations(
        false,
        visible,
        WindowInteractionMode::MoveResize
    ));
    assert!(!can_advance_window_animations(
        false,
        visible,
        WindowInteractionMode::Moving
    ));
    assert!(!can_advance_window_animations(
        false,
        visible,
        WindowInteractionMode::Sizing
    ));
    assert!(!can_advance_window_animations(
        true,
        visible,
        WindowInteractionMode::Idle
    ));

    let mut hidden_for_owner = visible;
    assert!(hidden_for_owner.hide_for_owner());
    assert!(!can_advance_window_animations(
        false,
        hidden_for_owner,
        WindowInteractionMode::Idle
    ));
}

#[test]
fn interactive_resize_emits_first_pending_frame_on_next_tick() {
    let mut throttle = ResizeFrameThrottle::default();
    throttle.begin();
    throttle.request();

    assert!(throttle.advance(1.0));
}

#[test]
fn interactive_resize_coalesces_requests_until_frame_interval_elapses() {
    let mut throttle = ResizeFrameThrottle::default();
    throttle.begin();
    throttle.request();
    assert!(throttle.advance(1.0));

    throttle.request();
    assert!(!throttle.advance(10.0));
    throttle.request();
    assert!(!throttle.advance(22.0));
    assert!(throttle.advance(1.0));
}

#[test]
fn interactive_resize_does_not_emit_without_a_pending_request() {
    let mut throttle = ResizeFrameThrottle::default();
    throttle.begin();

    assert!(!throttle.advance(33.0));
}

#[cfg(feature = "images-win32")]
#[test]
fn completed_images_repaint_only_matching_node_bounds() {
    use std::{collections::HashSet, sync::Arc};

    use crate::core::{ImageFit, ImageRequest, UiId, UiImageSource, UiNode, UiNodeKind, UiRect};

    let target_request = ImageRequest::new(UiImageSource::url("https://example.test/avatar.png"));
    let target_key = target_request.cache_key();
    let target_bounds = UiRect::new(12.0, 18.0, 68.0, 74.0);
    let target = Arc::new(
        UiNode::new(UiId::new("target-avatar"), UiNodeKind::Image, target_bounds)
            .image_request(target_request, ImageFit::Cover)
            .paint_bounds(target_bounds),
    );
    let other_bounds = UiRect::new(400.0, 300.0, 456.0, 356.0);
    let other = Arc::new(
        UiNode::new(UiId::new("other-avatar"), UiNodeKind::Image, other_bounds)
            .image_request(
                ImageRequest::new(UiImageSource::url("https://example.test/other.png")),
                ImageFit::Cover,
            )
            .paint_bounds(other_bounds),
    );

    assert_eq!(
        image_repaint_bounds(&[target, other], &HashSet::from([target_key])),
        vec![target_bounds]
    );
}
