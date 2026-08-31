use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn application_integration_maps_tray_commands_and_visibility() {
    let commands = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&commands);
    let registration = TrayRegistration {
        options: TrayOptions::new("test"),
        handler: Arc::new(move |_, command| {
            captured
                .lock()
                .expect("tray command capture poisoned")
                .push(command.to_owned());
        }),
    };
    let context = ApplicationContext::empty();
    let visible = Arc::new(Mutex::new(None));
    let captured_visibility = Arc::clone(&visible);

    dispatch_tray_action(
        &registration,
        &context,
        TrayAction::command_and_show_main("refresh"),
        move |value| {
            *captured_visibility
                .lock()
                .expect("tray visibility capture poisoned") = Some(value);
        },
    );

    assert_eq!(
        *commands.lock().expect("tray command capture poisoned"),
        ["refresh"]
    );
    assert_eq!(
        *visible.lock().expect("tray visibility capture poisoned"),
        Some(true)
    );
}
