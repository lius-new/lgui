use super::{host::is_activation_message, support::copy_wide};
use windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONUP;

#[test]
fn single_left_click_activates_the_tray_icon() {
    assert!(is_activation_message(WM_LBUTTONUP));
    assert!(!is_activation_message(
        windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDBLCLK
    ));
}

#[test]
fn notification_text_is_terminated_and_truncated_to_fit() {
    let mut buffer = [9_u16; 4];
    copy_wide(&mut buffer, "ABCDE");
    assert_eq!(buffer, ['A' as u16, 'B' as u16, 'C' as u16, 0]);
}
