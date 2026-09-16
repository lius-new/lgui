use lgui::prelude::*;
use lgui::WinitApplication;

fn app(cx: &mut RenderCx<'_, '_>) -> Element {
    let count = cx.state(0_i32);
    let increment = count.clone();

    stack(UiRect::new(0.0, 0.0, 360.0, 200.0), Axis::Vertical)
        .gap(12.0)
        .padding(EdgeInsets::all(24.0))
        .align(Align::Stretch)
        .content((
            text(
                UiRect::new(0.0, 0.0, 312.0, 56.0),
                format!("Count: {}", count.get()),
                TextStyle::new(Color(0xF4F7FA), -24.0, 700).centered(),
            ),
            button(
                UiRect::new(0.0, 0.0, 312.0, 48.0),
                "one up",
                ButtonStyle {
                    panel: VisualStyle::filled(Color(0x2FB8C5)).radius(6.0),
                    text: TextStyle::new(Color(0x071013), -18.0, 700).centered(),
                    hover_outset: (2.0, 2.0),
                },
            )
            .on_click(move |_| increment.update(|value| *value += 1)),
        ))
        .into()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Application::with_backend(WinitApplication::new(GraphicsPreference::Auto))
        .provide(RendererKind::Skia(GraphicsPreference::Auto))
        .memory_options(MemoryOptions::unbounded(
            ImageCachePolicy::WhileVisible,
            false,
        ))
        .window_options(
            WindowOptions::new("counter")
                .title("lgui counter")
                .size(Size::new(380.0, 240.0)),
        )
        .run(app)?;
    Ok(())
}
