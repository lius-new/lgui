use std::sync::Arc;

use crate::core::{component, context_provider, Element, RenderCx, RootComponent};

use super::ApplicationContext;

pub type AppView = Arc<
    dyn for<'scope, 'context> Fn(&mut RenderCx<'scope, 'context>) -> Element
        + Send
        + Sync
        + 'static,
>;

struct ApplicationRoot {
    context: ApplicationContext,
    view: AppView,
}

impl RootComponent for ApplicationRoot {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> Element {
        let viewport = cx.viewport();
        let view = self.view;
        let content = component(viewport, move |cx, _| view(cx)).key("lgui.application.root");
        context_provider(self.context, content)
    }
}

pub(crate) fn application_root_view(context: ApplicationContext, view: AppView) -> AppView {
    Arc::new(move |cx| {
        ApplicationRoot {
            context: context.clone(),
            view: Arc::clone(&view),
        }
        .render_root(cx)
    })
}

impl RootComponent for AppView {
    fn render_root(self, cx: &mut RenderCx<'_, '_>) -> Element {
        self(cx)
    }
}
