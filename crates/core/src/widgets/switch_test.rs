use super::*;
use crate::core::{context_provider, HostTreeBuilder, RenderCx, RootComponent, UiRuntime, UiScale};

struct ThemedSwitchRoot {
    theme: ThemeContext,
}

impl RootComponent for ThemedSwitchRoot {
    fn render_root(self, _cx: &mut RenderCx<'_, '_>) -> Element {
        context_provider(
            self.theme,
            Element::from(switch(UiRect::new(0.0, 0.0, 46.0, 24.0), false, |_| {})),
        )
    }
}

#[test]
fn switch_visuals_interpolate_between_stable_endpoints() {
    assert_eq!(mix_f32(4.0, 26.0, 0.0), 4.0);
    assert_eq!(mix_f32(4.0, 26.0, 1.0), 26.0);
    assert_eq!(
        mix_color(Color(0x000000), Color(0xFFFFFF), 0.5),
        Color(0x808080)
    );
    assert_eq!(smootherstep(0.0), 0.0);
    assert_eq!(smootherstep(1.0), 1.0);
}

#[test]
fn default_style_resolves_from_the_nearest_theme_context() {
    let mut tokens = ThemeTokens::default();
    tokens.colors.surface_raised = Color(0x123456);
    let ui = UiRuntime::new();
    let viewport = UiRect::new(0.0, 0.0, 46.0, 24.0);
    let interaction = ui.interaction_state();
    let mut builder = HostTreeBuilder::new();
    builder.mount(
        ThemedSwitchRoot {
            theme: ThemeContext::new(tokens),
        },
        viewport,
        &interaction,
        ui.animations(),
        ui.component_states(),
        ui.component_tree(),
        ui.contexts(),
        ui.hook_states(),
        ui.hook_updates(),
        ui.task_spawner(),
        ui.effects(),
        UiScale::ONE,
    );

    let tree = builder.finish();
    let track = tree
        .nodes()
        .iter()
        .find(|node| node.kind == crate::core::UiNodeKind::Panel && node.layout_rect == viewport)
        .expect("switch track");
    assert_eq!(track.style.fill, Some(Color(0x123456)));
}
