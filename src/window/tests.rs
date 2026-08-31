use std::sync::{Arc, Mutex};

use crate::{core::Size, platform::dpi::ScalePreference};

use super::*;

#[test]
fn native_titlebar_is_enabled_by_default_and_can_be_disabled() {
    assert!(WindowOptions::default().native_titlebar);
    assert!(WindowOptions::new("default").native_titlebar);
    assert!(
        !WindowOptions::new("custom")
            .native_titlebar(false)
            .native_titlebar
    );
}

#[test]
fn windows_are_visible_by_default_and_can_start_hidden() {
    assert!(WindowOptions::default().visible);
    assert!(!WindowOptions::new("background").visible(false).visible);
}

#[test]
fn window_size_constraints_are_optional_and_configurable() {
    let defaults = WindowOptions::new("default");
    assert_eq!(defaults.minimum_size, None);
    assert_eq!(defaults.maximum_size, None);

    let constrained = defaults
        .minimum_size(Size::new(640.0, 360.0))
        .maximum_size(Size::new(1920.0, 1080.0));
    assert_eq!(constrained.minimum_size, Some(Size::new(640.0, 360.0)));
    assert_eq!(constrained.maximum_size, Some(Size::new(1920.0, 1080.0)));
}

#[test]
fn auxiliary_window_options_are_declared_without_platform_handles() {
    let options = WindowOptions::new("friends")
        .owner("main")
        .title("Friends")
        .size(Size::new(292.0, 640.0))
        .minimum_size(Size::new(292.0, 360.0))
        .position(WindowPosition::AdjacentToOwner { gap: 1 })
        .transparent(true)
        .corner_radius(8)
        .background_memory_optimization(true);

    assert_eq!(options.id.as_str(), "friends");
    assert_eq!(options.owner.as_ref().map(WindowId::as_str), Some("main"));
    assert_eq!(options.position, WindowPosition::AdjacentToOwner { gap: 1 });
    assert!(options.transparent);
    assert!(options.background_memory_optimization);
}

#[test]
fn window_manager_marshals_the_complete_command_model() {
    let manager = WindowManager::new();
    let commands = Arc::new(Mutex::new(Vec::new()));
    let recorded = Arc::clone(&commands);
    manager.install(move |command| {
        let label = match command {
            WindowCommand::Show { options, .. } => format!("show:{}", options.id.as_str()),
            WindowCommand::Hide(id) => format!("hide:{}", id.as_str()),
            WindowCommand::Toggle { options, .. } => {
                format!("toggle:{}", options.id.as_str())
            }
            WindowCommand::Close(id) => format!("close:{}", id.as_str()),
            WindowCommand::RequestClose(id) => format!("request-close:{}", id.as_str()),
            WindowCommand::Minimize(id) => format!("minimize:{}", id.as_str()),
            WindowCommand::SetScalePreference(ScalePreference::Auto) => "scale:auto".into(),
            WindowCommand::SetScalePreference(ScalePreference::Multiplier(value)) => {
                format!("scale:{value}")
            }
            WindowCommand::SetMode { id, mode } => {
                format!("mode:{}:{mode:?}", id.as_str())
            }
            WindowCommand::Input { id, .. } => format!("input:{}", id.as_str()),
            WindowCommand::Exit => "exit".into(),
        };
        recorded.lock().expect("command log poisoned").push(label);
    });

    manager.show(WindowOptions::new("friends"), |_| {
        crate::core::content_text("friends")
    });
    manager.hide("friends");
    manager.toggle(WindowOptions::new("friends"), |_| {
        crate::core::content_text("friends")
    });
    manager.set_scale_preference(ScalePreference::Multiplier(0.9));
    manager.set_mode("friends", WindowMode::Fullscreen);
    manager.request_close("friends");
    manager.close("friends");
    manager.exit();

    assert_eq!(
        *commands.lock().expect("command log poisoned"),
        [
            "show:friends",
            "hide:friends",
            "toggle:friends",
            "scale:0.9",
            "mode:friends:Fullscreen",
            "request-close:friends",
            "close:friends",
            "exit",
        ]
    );
}
